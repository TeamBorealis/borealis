import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { sequenceDie } from "../lib/dice";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { LONG_PRESS_MS } from "../pages/play/rolls/usePress";
import { setDieForTests } from "../pages/play/rolls/rollStore";
import { renderApp, seededId } from "./utils";

// GitHub #14 (Tjop Vaoxad) and #1 (kirktobridge), decision U16(b): the Play tab as a table-session sheet. Abilities,
// skills, attacks and spells on sub-tabs; a tap rolls, a long press (touch), a right-click or the arrow button (mouse,
// keyboard) offers advantage and disadvantage; names open descriptions (pane on wide screens, sheet on phones); Cast
// spends a slot; a recent-rolls history. Dice are scripted so every total is known.

const LAYOUTS = [["desktop", false], ["phone", true]] as const;

// jsdom has no PointerEvent: a MouseEvent subclass carries pointerType (as in b48-lists-controls.test.tsx).
if (typeof window.PointerEvent === "undefined") {
  class FakePointerEvent extends MouseEvent {
    pointerId: number;
    pointerType: string;
    constructor(type: string, init: PointerEventInit = {}) { super(type, init); this.pointerId = init.pointerId ?? 1; this.pointerType = init.pointerType ?? "mouse"; }
  }
  (window as unknown as { PointerEvent: typeof FakePointerEvent }).PointerEvent = FakePointerEvent;
}

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

const result = () => screen.getByRole("status", { name: "Roll result" });
const tab = async (user: ReturnType<typeof userEvent.setup>, name: string) => user.click(await screen.findByRole("tab", { name }));

describe.each(LAYOUTS)("rolling (%s)", (_layout, phone) => {
  it("a tap rolls a check with the snapshot's bonus and the result names the dice", async () => {
    // Arrange
    const { user, snap } = await openPlay("Seraphine", { phone, dice: [14] });
    await tab(user, "Abilities");
    const str = snap.abilities.check.str;
    const sign = str >= 0 ? `+${str}` : `${str}`;

    // Act
    await user.click(await screen.findByRole("button", { name: `Roll Strength check, ${sign}` }));

    // Assert
    await waitFor(() => expect(result().textContent).toContain(`Strength check ${14 + str}`));
    expect(result().textContent).toContain(`d20 [14] ${sign} = ${14 + str}`);
  });

  it("a right-click offers advantage, normal and disadvantage; advantage keeps the higher die", async () => {
    const { user, snap } = await openPlay("Seraphine", { phone, dice: [14, 7] });
    await tab(user, "Abilities");
    const skill = snap.stats.skills.find((k) => k.name === "Stealth")!;
    const roll = await screen.findByRole("button", { name: new RegExp(`^Roll Stealth check, [+-]${Math.abs(skill.bonus)}`) });
    fireEvent.contextMenu(roll);
    const menu = await screen.findByRole("menu", { name: "Roll Stealth check" });
    expect(within(menu).getAllByRole("menuitem").map((m) => m.textContent)).toEqual(["Advantage", "Normal", "Disadvantage"]);
    await user.click(within(menu).getByRole("menuitem", { name: "Advantage" }));
    await waitFor(() => expect(result().textContent).toContain(`= ${14 + skill.bonus}, advantage`));
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("a long press opens the menu without rolling; the touch's click is not a roll", async () => {
    const { user } = await openPlay("Seraphine", { phone, dice: [3, 18] });
    await tab(user, "Abilities");
    const roll = await screen.findByRole("button", { name: /^Roll Wisdom save, / });
    fireEvent.pointerDown(roll, { pointerType: "touch", clientX: 10, clientY: 10 });
    await new Promise((r) => setTimeout(r, LONG_PRESS_MS + 60));
    fireEvent.pointerUp(roll, { pointerType: "touch" });
    fireEvent.click(roll);
    const menu = await screen.findByRole("menu", { name: "Roll Wisdom save" });
    expect(result().textContent).toBe("");
    await user.click(within(menu).getByRole("menuitem", { name: "Disadvantage" }));
    await waitFor(() => expect(result().textContent).toContain("d20 [3, 18]"));
    expect(result().textContent).toContain("disadvantage");
  });

  it("a finger that moves (a scroll) opens no menu", async () => {
    const { user } = await openPlay("Seraphine", { phone });
    await tab(user, "Abilities");
    const roll = await screen.findByRole("button", { name: /^Roll Wisdom save, / });
    fireEvent.pointerDown(roll, { pointerType: "touch", clientX: 10, clientY: 10 });
    fireEvent.pointerMove(roll, { pointerType: "touch", clientX: 10, clientY: 40 });
    await new Promise((r) => setTimeout(r, LONG_PRESS_MS + 60));
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("rolling from the keyboard", () => {
  it("Shift+F10 and the arrow button open the menu; arrows move, Enter rolls, Escape gives focus back", async () => {
    const { user } = await openPlay("Seraphine", { dice: [11] });
    await tab(user, "Abilities");
    const roll = await screen.findByRole("button", { name: /^Roll Initiative, / });
    roll.focus();
    await user.keyboard("{Shift>}{F10}{/Shift}");
    const menu = await screen.findByRole("menu", { name: "Roll Initiative" });
    expect(document.activeElement?.textContent).toBe("Advantage");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(roll);
    expect(menu.isConnected).toBe(false);

    const more = screen.getByRole("button", { name: "More: Roll Initiative" });
    more.focus();
    await user.keyboard("{Enter}");
    await screen.findByRole("menu", { name: "Roll Initiative" });
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement?.textContent).toBe("Normal");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(result().textContent).toContain("d20 [11]"));
    expect(result().textContent).not.toContain("advantage");
  });
});

describe.each(LAYOUTS)("attacks (%s)", (_layout, phone) => {
  it("a roll in the crit range offers doubled damage dice", async () => {
    const { user, snap } = await openPlay("Brannoc", { phone, dice: [19, 5, 3] });
    await tab(user, "Attacks");
    const sword = snap.attacks.find((a) => a.name === "Longsword")!;
    await user.click(await screen.findByRole("button", { name: new RegExp(`^Roll Longsword attack, \\+${sword.toHit}`) }));
    await waitFor(() => expect(result().textContent).toContain(`Longsword attack ${19 + sword.toHit!}`));
    await user.click(within(screen.getByTestId("roll-result")).getByRole("button", { name: "Roll critical damage" }));
    await waitFor(() => expect(result().textContent).toContain("Longsword damage, critical"));
    expect(result().textContent).toMatch(/2d8 \[5, 3\] \+\d+ = \d+, critical/);
  });

  it("a normal hit offers the plain damage, and the damage button rolls on its own", async () => {
    const { user } = await openPlay("Brannoc", { phone, dice: [12, 4, 6] });
    await tab(user, "Attacks");
    await user.click(await screen.findByRole("button", { name: /^Roll Longsword attack, / }));
    await user.click(await within(screen.getByTestId("roll-result")).findByRole("button", { name: "Roll damage" }));
    await waitFor(() => expect(result().textContent).toMatch(/1d8 \[4\] \+\d+ = \d+/));
    await user.click(screen.getByRole("button", { name: "Roll Handaxe damage, 1d6+3 slashing" }));
    await waitFor(() => expect(result().textContent).toContain("1d6 [6] +3 = 9"));
  });

  it("the attack's name opens its description, not a roll", async () => {
    const { user } = await openPlay("Brannoc", { phone });
    await tab(user, "Attacks");
    await user.click(await screen.findByRole("button", { name: "Handaxe, description" }));
    if (phone) {
      const sheet = await screen.findByRole("dialog", { name: "Handaxe" });
      expect(within(sheet).getByText("Thrown (range 20/60). Light.")).toBeTruthy();
      await user.click(within(sheet).getByRole("button", { name: "Close" }));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    } else {
      const pane = screen.getByRole("complementary", { name: "Attacks details" });
      expect(within(pane).getByText("Thrown (range 20/60). Light.")).toBeTruthy();
      expect(screen.queryByRole("dialog")).toBeNull();
    }
    expect(result().textContent).toBe("");
  });
});

describe.each(LAYOUTS)("spells (%s)", (_layout, phone) => {
  it("lists the spells ready to cast by level; Cast spends a level 1 slot", async () => {
    const { user, api, characterId } = await openPlay("Seraphine", { phone });
    await tab(user, "Spells");
    const level1 = await screen.findByRole("list", { name: "Wizard level 1" });
    const names = within(level1).getAllByRole("button", { name: /, description$/ }).map((b) => b.getAttribute("aria-label"));
    expect(names).toEqual(["Mage Armor, description", "Magic Missile, description", "Shield, description", "Thunderwave, description"]);
    expect(screen.getByText(/3\/3 slots/)).toBeTruthy();
    await user.click(within(level1).getByRole("button", { name: "Cast Magic Missile, level 1 slot" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId })).play.spellSlots[0]!.used).toBe(1));
    expect(await screen.findByText(/2\/3 slots/)).toBeTruthy();
    expect(await screen.findByText("Cast Magic Missile with a level 1 slot.")).toBeTruthy();
  });

  it("casting a concentration spell concentrates on it; with no slots left Cast is off and the ritual stays in the menu", async () => {
    const prep = (api: MockApi, id: string) => api.setSpellPrepared({ characterId: id, classSlotId: "c0", spellId: "detect-magic", prepared: true });
    const { user } = await openPlay("Seraphine", { phone, prep });
    await tab(user, "Spells");
    const level1 = await screen.findByRole("list", { name: "Wizard level 1" });
    await user.click(within(level1).getByRole("button", { name: "Cast Detect Magic, level 1 slot" }));
    expect(await screen.findByRole("region", { name: "Concentration" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Concentration" }).textContent).toContain("Detect Magic");
    for (let i = 0; i < 2; i++) {
      await user.click(within(level1).getByRole("button", { name: "Cast Shield, level 1 slot" }));
      await screen.findByText(new RegExp(`${1 - i}/3 slots`));
    }
    expect((within(level1).getByRole("button", { name: "Cast Shield: no slots left" }) as HTMLButtonElement).disabled).toBe(true);
    await user.click(within(level1).getByRole("button", { name: "Cast Detect Magic, choose a slot" }));
    const menu = await screen.findByRole("menu", { name: "Cast Detect Magic" });
    expect(within(menu).getAllByRole("menuitem").map((m) => m.textContent)).toEqual(["As a ritual (no slot)"]);
  });

  it("a spell's name opens its description", async () => {
    const { user } = await openPlay("Seraphine", { phone });
    await tab(user, "Spells");
    await user.click(await screen.findByRole("button", { name: "Magic Missile, description" }));
    const where = phone ? await screen.findByRole("dialog", { name: "Magic Missile" }) : screen.getByRole("complementary", { name: "Spells details" });
    expect(await within(where).findByText(/Level 1/)).toBeTruthy();
  });
});

describe("history, hit dice, resource dice and exhaustion", () => {
  it("keeps the session's rolls, newest first, and Clear empties them", async () => {
    const { user } = await openPlay("Seraphine", { dice: [5, 9] });
    await tab(user, "Abilities");
    await user.click(await screen.findByRole("button", { name: /^Roll Strength check, / }));
    await user.click(screen.getByRole("button", { name: /^Roll Dexterity check, / }));
    await user.click(screen.getByRole("button", { name: "Recent rolls (2)" }));
    const dialog = await screen.findByRole("dialog", { name: "Recent rolls" });
    const rows = within(within(dialog).getByRole("list", { name: "Recent rolls" })).getAllByRole("listitem");
    expect(rows.map((r) => r.textContent?.split("d20")[0])).toEqual(["Dexterity check", "Strength check"]);
    await user.click(within(dialog).getByRole("button", { name: "Clear" }));
    expect(within(dialog).getByText("No rolls yet")).toBeTruthy();
  });

  it("rolls a hit die with the Constitution modifier, and a resource's die, spending a use from the menu", async () => {
    const { user } = await openPlay("Seraphine", { dice: [4, 6, 2] });
    await user.click(await screen.findByRole("button", { name: "Roll a Wizard hit die, 1d6+2" }));
    await waitFor(() => expect(result().textContent).toContain("Wizard hit die 6"));
    expect(result().textContent).toContain("1d6 [4] +2 = 6");
    await user.click(screen.getByRole("button", { name: "Roll Inspiration Die, 1d6" }));
    await waitFor(() => expect(result().textContent).toContain("1d6 [6] = 6"));
    expect(screen.getByRole("status", { name: "Inspiration Die: 3 of 3 uses left" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "More: Roll Inspiration Die" }));
    await user.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Spend one and roll" }));
    await waitFor(() => expect(screen.getByRole("status", { name: "Inspiration Die: 2 of 3 uses left" })).toBeTruthy());
    await waitFor(() => expect(result().textContent).toContain("1d6 [2] = 2"));
  });

  it("2024 Exhaustion: the note says the penalty is in every d20 roll, and the bonuses already hold it", async () => {
    const prep = (api: MockApi, id: string) => api.setExhaustion({ characterId: id, level: 2 });
    const { user, snap } = await openPlay("Seraphine", { dice: [10], prep });
    await tab(user, "Abilities");
    expect(await screen.findByText("Exhaustion 2: -4 is in every d20 roll.")).toBeTruthy();
    const str = snap.abilities.check.str;
    expect(str).toBe(snap.abilities.modifier.str - 4);
    await user.click(screen.getByRole("button", { name: new RegExp(`^Roll Strength check, ${str}`) }));
    await waitFor(() => expect(result().textContent).toContain(`d20 [10] ${str} = ${10 + str}`));
  });

  it("a feature's name opens its description in a sheet on phones", async () => {
    const { user } = await openPlay("Seraphine", { phone: true });
    await tab(user, "Traits");
    await user.click(await screen.findByRole("button", { name: /^Mock Feature/ }));
    const sheet = await screen.findByRole("dialog", { name: "Mock Feature" });
    expect(within(sheet).getByText(/feature description/)).toBeTruthy();
  });

  it("a character without spellcasting has no Spells tab", async () => {
    await openPlay("Brannoc");
    await screen.findByRole("tab", { name: "Abilities" });
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual(["Play", "Abilities", "Attacks", "Traits"]);
  });
});
