import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { sequenceDie } from "../lib/dice";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { parseAmount } from "../pages/play/AmountControl";
import { setDieForTests } from "../pages/play/rolls/rollStore";
import { renderApp, seededId } from "./utils";

// B-194 (decision PL1, suggested by Egorobi on Discord): current and temporary hit points on Play, hit dice spent and
// recovered through the rests, and trackers that change by any typed amount. Seraphine is a 2024 Wizard 2, Brannoc a 2014
// Fighter 3. Dice are scripted so every heal is known. Every check runs on the desktop and the phone layout.

const LAYOUTS = [["desktop", false], ["phone", true]] as const;

afterEach(() => { setPhoneForTests(false); setPhoneOsForTests(null); setDieForTests(null); });

async function openPlay(name: string, opts: { phone?: boolean; dice?: number[]; prep?: (api: MockApi, id: string) => Promise<unknown> } = {}) {
  if (opts.phone) { setPhoneForTests(true); setPhoneOsForTests("ios"); }
  setDieForTests(sequenceDie(opts.dice ?? []));
  const api = new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
  const characterId = await seededId(api, name);
  await opts.prep?.(api, characterId);
  const snap = await api.getSnapshot({ characterId });
  renderApp("/play", { api, openCharacterId: characterId });
  return { api, characterId, snap, user: userEvent.setup() };
}

type User = ReturnType<typeof userEvent.setup>;
const hpText = () => screen.getByRole("status", { name: /^Hit points: \d+ of \d+$/ }).textContent;
const tempText = () => screen.getByRole("status", { name: /^Temporary hit points: \d+$/ }).textContent;
const amountField = (name = "Hit point amount") => screen.getByRole("spinbutton", { name });
/** Types `amount` in the field (one change event, so a slow run stays inside its own test) and presses `command`. */
async function enter(user: User, amount: string, command: string, field?: string) {
  fireEvent.change(amountField(field), { target: { value: amount } });
  await user.click(screen.getByRole("button", { name: command }));
}

describe("the typed amount", () => {
  it("takes whole numbers up to the limit only", () => {
    expect(parseAmount("7", 10)).toBe(7);
    expect(parseAmount(" 0 ", 10)).toBe(0);
    expect(parseAmount("11", 10)).toBeNull();
    expect(parseAmount("2.5", 10)).toBeNull();
    expect(parseAmount("-3", 10)).toBeNull();
    expect(parseAmount("", 10)).toBeNull();
  });
});

describe.each(LAYOUTS)("hit points card (%s)", (_layout, phone) => {
  it("starts at full hit points, and -1/+1 step", async () => {
    const { user, snap } = await openPlay("Seraphine", { phone });
    const max = snap.play.hitPoints.max;
    expect(max).toBe(snap.stats.hpMax);
    await waitFor(() => expect(hpText()).toBe(String(max)));
    expect(tempText()).toBe("0");
    expect((screen.getByRole("button", { name: "Regain 1 hit point" }) as HTMLButtonElement).disabled).toBe(true);
    await user.click(screen.getByRole("button", { name: "Take 1 damage" }));
    await waitFor(() => expect(hpText()).toBe(String(max - 1)));
    await user.click(screen.getByRole("button", { name: "Regain 1 hit point" }));
    await waitFor(() => expect(hpText()).toBe(String(max)));
  });

  it("damage takes temporary hit points first and stops at 0; healing stops at the maximum", async () => {
    const { user, snap } = await openPlay("Seraphine", { phone });
    const max = snap.play.hitPoints.max;
    await waitFor(() => expect(hpText()).toBe(String(max)));
    // 3 temporary hit points, then 5 damage: the 3 go first, 2 come off hit points
    await enter(user, "3", "Set temporary hit points");
    await waitFor(() => expect(tempText()).toBe("3"));
    expect((amountField() as HTMLInputElement).value).toBe("");
    await enter(user, "5", "Take damage");
    await waitFor(() => expect(hpText()).toBe(String(max - 2)));
    expect(tempText()).toBe("0");
    await enter(user, "50", "Regain hit points");
    await waitFor(() => expect(hpText()).toBe(String(max)));
    await enter(user, "999", "Take damage");
    await waitFor(() => expect(hpText()).toBe("0"));
    expect((screen.getByRole("button", { name: "Take 1 damage" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("temporary hit points replace each other and End clears them; a non-number enables nothing", async () => {
    const { user } = await openPlay("Seraphine", { phone });
    await screen.findByRole("status", { name: /^Hit points: / });
    await enter(user, "8", "Set temporary hit points");
    await waitFor(() => expect(tempText()).toBe("8"));
    await enter(user, "5", "Set temporary hit points");
    await waitFor(() => expect(tempText()).toBe("5"));
    await user.click(screen.getByRole("button", { name: "End temporary hit points" }));
    await waitFor(() => expect(tempText()).toBe("0"));
    fireEvent.change(amountField(), { target: { value: "2.5" } });
    for (const name of ["Take damage", "Regain hit points", "Set temporary hit points"]) {
      expect((screen.getByRole("button", { name }) as HTMLButtonElement).disabled).toBe(true);
    }
  });
});

describe.each(LAYOUTS)("trackers by any amount (%s)", (_layout, phone) => {
  it("a typed amount uses and restores a tracker, capped at what is left; the +1/-1 steps stay", async () => {
    const prep = (api: MockApi, id: string) => api.addTracker({ characterId: id, name: "Luck", max: 20, reset: "never" });
    const { user } = await openPlay("Seraphine", { phone, prep });
    const status = () => screen.getByRole("status", { name: /^Luck: \d+ of 20 left$/ }).textContent;
    await waitFor(() => expect(status()).toBe("20/20"));
    await user.click(screen.getByRole("button", { name: "Spend Luck" }));
    await waitFor(() => expect(status()).toBe("19/20"));
    await user.click(screen.getByRole("button", { name: "Change Luck by an amount" }));
    await enter(user, "7", "Use an amount of Luck", "Luck amount");
    await waitFor(() => expect(status()).toBe("12/20"));
    await enter(user, "50", "Use an amount of Luck", "Luck amount");
    await waitFor(() => expect(status()).toBe("0/20"));
    expect((screen.getByRole("button", { name: "Use an amount of Luck" }) as HTMLButtonElement).disabled).toBe(true);
    await enter(user, "5", "Restore an amount of Luck", "Luck amount");
    await waitFor(() => expect(status()).toBe("5/20"));
    await user.click(screen.getByRole("button", { name: "Restore Luck" }));
    await waitFor(() => expect(status()).toBe("6/20"));
  });

  it("a tracker that counts up adds and removes any amount", async () => {
    const prep = (api: MockApi, id: string) => api.addTracker({ characterId: id, name: "Doom", max: 5, reset: "long_rest", countsUp: true });
    const { user } = await openPlay("Seraphine", { phone, prep });
    const status = () => screen.getByRole("status", { name: /^Doom: \d+ of 5$/ }).textContent;
    await waitFor(() => expect(status()).toBe("0/5"));
    await user.click(screen.getByRole("button", { name: "Change Doom by an amount" }));
    await enter(user, "3", "Add an amount of Doom", "Doom amount");
    await waitFor(() => expect(status()).toBe("3/5"));
    await enter(user, "2", "Remove an amount of Doom", "Doom amount");
    await waitFor(() => expect(status()).toBe("1/5"));
  });
});

describe.each(LAYOUTS)("hit dice through the rests (%s)", (_layout, phone) => {
  it("Spend rolls a die plus the Constitution modifier and heals by it; Restore gives the die back", async () => {
    const prep = (api: MockApi, id: string) => api.takeDamage({ characterId: id, amount: 10 });
    const { user, snap } = await openPlay("Seraphine", { phone, dice: [4], prep });
    const max = snap.play.hitPoints.max;
    const con = snap.abilities.modifier.con;
    const dice = () => screen.getByRole("status", { name: /^Wizard hit dice: \d of 2 left$/ }).textContent;
    await waitFor(() => expect(dice()).toBe("2/2"));
    expect(hpText()).toBe(String(max - 10));
    await user.click(screen.getByRole("button", { name: "Spend a Wizard hit die" }));
    await waitFor(() => expect(dice()).toBe("1/2"));
    await waitFor(() => expect(hpText()).toBe(String(max - 10 + 4 + con)));
    expect(screen.getByRole("status", { name: "Roll result" }).textContent).toContain(`1d6 [4] +${con}`);
    await user.click(screen.getByRole("button", { name: "Restore a Wizard hit die" }));
    await waitFor(() => expect(dice()).toBe("2/2"));
    expect(hpText()).toBe(String(max - 10 + 4 + con));
  });

  it("a Short Rest offers hit dice when hurt, keeps resources' rules, and a 2024 Long Rest brings every hit point and die back", async () => {
    const prep = async (api: MockApi, id: string) => {
      await api.takeDamage({ characterId: id, amount: 12 });
      await api.setTempHitPoints({ characterId: id, amount: 4 });
    };
    const { user, snap } = await openPlay("Seraphine", { phone, dice: [1, 2], prep });
    const max = snap.play.hitPoints.max;
    const con = snap.abilities.modifier.con;
    await waitFor(() => expect(hpText()).toBe(String(max - 12)));
    await user.click(screen.getByRole("button", { name: "Short Rest" }));
    const dialog = await screen.findByRole("dialog", { name: "Short Rest" });
    expect(within(dialog).getByRole("status", { name: `Hit points: ${max - 12} of ${max}` })).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Spend a Wizard hit die, 2 of 2 left" }));
    await waitFor(() => expect(within(dialog).getByRole("status", { name: `Hit points: ${max - 12 + 1 + con} of ${max}` })).toBeTruthy());
    expect(within(dialog).getByText(/^Wizard hit die: 1d6 \[1\]/)).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Spend a Wizard hit die, 1 of 2 left" }));
    await waitFor(() => expect(within(dialog).getByRole("status", { name: `Hit points: ${max - 12 + 3 + 2 * con} of ${max}` })).toBeTruthy());
    expect((within(dialog).getByRole("button", { name: "Spend a Wizard hit die, 0 of 2 left" }) as HTMLButtonElement).disabled).toBe(true);
    await user.click(within(dialog).getByRole("button", { name: "Done" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Short Rest" })).toBeNull());
    expect(tempText()).toBe("4");
    await user.click(screen.getByRole("button", { name: "Long Rest" }));
    await waitFor(() => expect(hpText()).toBe(String(max)));
    expect(tempText()).toBe("0");
    expect(screen.getByRole("status", { name: /^Wizard hit dice: / }).textContent).toBe("2/2");
  });

  it("a Short Rest at full hit points offers nothing", async () => {
    const { user } = await openPlay("Seraphine", { phone });
    await screen.findByRole("status", { name: /^Hit points: / });
    await user.click(screen.getByRole("button", { name: "Short Rest" }));
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByRole("dialog", { name: "Short Rest" })).toBeNull();
  });

  it("a 2014 Long Rest brings half the total hit dice back (at least one)", async () => {
    const prep = async (api: MockApi, id: string) => {
      for (const roll of [1, 1, 1]) await api.spendHitDie({ characterId: id, classId: "fighter", roll });
      await api.takeDamage({ characterId: id, amount: 5 });
    };
    const { user, snap } = await openPlay("Brannoc", { phone, prep });
    const dice = () => screen.getByRole("status", { name: /^Fighter hit dice: / }).textContent;
    await waitFor(() => expect(dice()).toBe("0/3"));
    await user.click(screen.getByRole("button", { name: "Long Rest" }));
    await waitFor(() => expect(dice()).toBe("1/3"));
    expect(hpText()).toBe(String(snap.play.hitPoints.max));
  });
});
