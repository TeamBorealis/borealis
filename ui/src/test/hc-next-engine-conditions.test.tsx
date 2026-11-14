import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { renderApp, seededId } from "./utils";

// HC1 (item HtCaM): the Play tab's Conditions card lists the conditions the character can switch on under Exhaustion,
// each with a switch. Switched on, a condition is tagged Active and kept (the mock, as the host, keeps it through a
// rest); its row selects it for the description pane, which shows its text and its effects.

const LAYOUTS = [["desktop", false], ["phone", true]] as const;

afterEach(() => { setPhoneForTests(false); setPhoneOsForTests(null); });

async function openPlay(phone: boolean) {
  if (phone) { setPhoneForTests(true); setPhoneOsForTests("ios"); }
  const api = new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
  const characterId = await seededId(api, "Seraphine");
  renderApp("/play", { api, openCharacterId: characterId });
  return { api, characterId, user: userEvent.setup() };
}

const row = async (name: string): Promise<HTMLElement> => {
  const list = await screen.findByRole("list", { name: "Conditions" });
  return within(list).getByRole("switch", { name }).closest("li") as HTMLElement;
};

describe.each(LAYOUTS)("conditions on the Play tab (%s)", (_layout, phone) => {
  it("lists the conditions without Exhaustion, all off; a switch turns one on and a rest keeps it", async () => {
    const { api, characterId, user } = await openPlay(phone);
    const list = await screen.findByRole("list", { name: "Conditions" });
    const names = within(list).getAllByRole("switch").map((s) => s.getAttribute("aria-label") ?? (s as HTMLInputElement).labels?.[0]?.textContent);
    expect(names).toHaveLength(14);
    expect(within(list).queryByRole("switch", { name: "Exhaustion" })).toBeNull();
    const prone = within(await row("Prone")).getByRole("switch", { name: "Prone" }) as HTMLInputElement;
    expect(prone.checked).toBe(false);
    await user.click(prone);
    await waitFor(() => expect(prone.checked).toBe(true));
    expect(within(await row("Prone")).getByText("Active")).toBeTruthy();
    const snap = await api.longRest({ characterId });
    expect(snap.play.conditions.filter((c) => c.on).map((c) => c.name)).toEqual(["Prone"]);
  });

  it("selecting a condition shows its text in the description", async () => {
    const { user } = await openPlay(phone);
    await user.click(within(await row("Grappled")).getByRole("button", { name: /^Grappled/ }));
    expect(await screen.findByText("Your Speed is 0.")).toBeTruthy();
  });
});

describe("conditions on the mock, as on the host", () => {
  it("only a listed condition is switched on; any id is switched off", async () => {
    const api = new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
    const characterId = await seededId(api, "Seraphine");
    await expect(api.setCondition({ characterId, conditionId: "ID_2024_CONDITION_EXHAUSTION", on: true })).rejects.toMatchObject({ code: "not_found" });
    const on = await api.setCondition({ characterId, conditionId: "ID_2024_CONDITION_PRONE", on: true });
    expect(on.play.conditions.find((c) => c.id === "ID_2024_CONDITION_PRONE")?.on).toBe(true);
    const off = await api.setCondition({ characterId, conditionId: "ID_GONE", on: false });
    expect(off.play.conditions.filter((c) => c.on).map((c) => c.id)).toEqual(["ID_2024_CONDITION_PRONE"]);
  });
});
