import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { CharacterSnapshot, Page, SelectionSlot, SlotOption, SlotOptionsArgs } from "../api/contract";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { abilityOptionRows, describedAbilityOption, parseAbilityOption } from "../pages/builder/abilityOption";
import { parseRules, readCss } from "./cssRules";
import { renderApp, seededId } from "./utils";

// Tester B5 (B-130, TV2): a background's ability-score options ("Intelligence +1, Charisma +2") had no description, so the
// tester picked one without knowing what it did. On a phone, describing an option shows each ability's new score and modifier.
const SLOT_ID = "bg-abil";
const OPTIONS = ["Intelligence +1, Charisma +2", "Intelligence +1, Wisdom +1, Charisma +1"];

const listSlot = (chosen: string | null): SelectionSlot => ({
  id: SLOT_ID, tab: "background", groupId: null, groupLabel: null, classSlotId: null, title: "Ability Scores (Acolyte)", elementType: "list",
  ordinal: 1, of: 1, required: true, locked: false, requiredLevel: null, allowReplace: false, hint: null, gainedOnLevel: 1, replaced: [],
  emptyReason: null, problem: null, prerequisitesUnmet: false,
  chosen: chosen ? { id: String(OPTIONS.indexOf(chosen) + 1), name: chosen, type: "list", source: "" } : null,
} as unknown as SelectionSlot);

const abilities = (over: Partial<Record<"int" | "wis" | "cha", number>> = {}) => {
  const final = { str: 15, dex: 14, con: 13, int: 8, wis: 10, cha: 8, ...over };
  const modifier = Object.fromEntries(Object.entries(final).map(([k, v]) => [k, Math.floor((v - 10) / 2)]));
  return { final, modifier, cap: 20 } as unknown as CharacterSnapshot["abilities"];
};

describe("background ability options are described with the resulting scores (B5)", () => {
  it("reads an option's increases", () => {
    expect(parseAbilityOption("Intelligence +1, Charisma +2")).toEqual({ int: 1, cha: 2 });
    expect(parseAbilityOption("Strength +1, Dexterity +1, Constitution +1")).toEqual({ str: 1, dex: 1, con: 1 });
    expect(parseAbilityOption("Lawful Good")).toBeNull();
  });

  it("each raised ability shows its score and modifier now and after the option", () => {
    const rows = abilityOptionRows(abilities(), listSlot(null), "Intelligence +1, Charisma +2");
    expect(rows).toEqual([
      { ability: "int", name: "Intelligence", from: 8, to: 9, modFrom: -1, modTo: -1 },
      { ability: "cha", name: "Charisma", from: 8, to: 10, modFrom: -1, modTo: 0 },
    ]);
  });

  it("an option already chosen in the slot is taken out first, so other options compare from the same scores", () => {
    // INT 9 and CHA 10 already include the chosen "+1 INT, +2 CHA".
    const chosen = listSlot("Intelligence +1, Charisma +2");
    const rows = abilityOptionRows(abilities({ int: 9, cha: 10 }), chosen, "Intelligence +1, Wisdom +1, Charisma +1");
    expect(rows.map((r) => [r.name, r.from, r.to])).toEqual([["Intelligence", 8, 9], ["Wisdom", 10, 11], ["Charisma", 8, 9]]);
  });

  it("only ability options get the computed pane: elements and other list entries keep the usual one", () => {
    const slot = listSlot(null);
    const trait = { ...listSlot(null), id: "trait", title: "Personality Trait" } as SelectionSlot;
    const choice = (slotId: string, name: string) => ({ slotId, name, choose: () => undefined });
    expect(describedAbilityOption({ id: "1", isElement: false, choice: choice(SLOT_ID, OPTIONS[0]!) }, [slot])).toEqual({ slot, text: OPTIONS[0] });
    expect(describedAbilityOption({ id: "1", isElement: false, choice: choice("trait", "I am always polite.") }, [trait])).toBeNull();
    expect(describedAbilityOption({ id: "ID_X", isElement: true }, [slot])).toBeNull();
    // the chosen row has no Choose action: its slot is the one holding it
    const held = listSlot(OPTIONS[1]!);
    expect(describedAbilityOption({ id: "2", isElement: false }, [held])).toEqual({ slot: held, text: OPTIONS[1] });
  });

  it("the Choose bar takes no height of its own on phones, so it never moves the page when it shows or goes", () => {
    const rules = parseRules(readCss("styles/phone/lists.css"));
    const bar = rules.find((r) => r.selectors.includes("html.phone .bp-choose"))?.body ?? "";
    const page = rules.find((r) => r.selectors.includes("html.phone .bp"))?.body ?? "";
    expect(bar).toMatch(/height: var\(--bp-choose-h\)/);
    expect(bar).toMatch(/margin-bottom: calc\(-1 \* var\(--bp-choose-h\)\)/);
    expect(page).toMatch(/padding-bottom: var\(--bp-choose-h\)/);
  });
});

/** A 2024 background with an ability-score list pick, as the engine emits it. */
class ListSlotApi extends MockApi {
  override async loadCharacter(args: { characterId: string }): Promise<CharacterSnapshot> {
    return withList(await super.loadCharacter(args));
  }
  override async getSnapshot(args: { characterId: string }): Promise<CharacterSnapshot> {
    return withList(await super.getSnapshot(args));
  }
  override async getSlotOptions(a: SlotOptionsArgs): Promise<Page<SlotOption>> {
    if (a.slotId !== SLOT_ID) return super.getSlotOptions(a);
    const items = OPTIONS.map((name, i) => ({ id: String(i + 1), name, type: "list", source: "", sourceName: "", shortDescription: "", prerequisites: null, rulesets: ["dnd2024"] }) as unknown as SlotOption);
    return { total: items.length, items };
  }
}
const withList = (s: CharacterSnapshot): CharacterSnapshot =>
  ({ ...s, slots: [...s.slots.filter((x) => x.tab !== "background"), listSlot(null)], abilities: { ...s.abilities, ...abilities() } });

afterEach(() => setPhoneForTests(false));

describe("Build > Background on a phone (B5)", () => {
  it("describing a background ability option shows that option and the resulting scores", async () => {
    setPhoneForTests(true);
    const user = userEvent.setup();
    const api = new ListSlotApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/builder/background", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: /^Intelligence \+1, Charisma \+2/ }));
    const pane = screen.getByRole("complementary", { name: "Description" });
    expect(within(pane).getByRole("heading", { name: "Intelligence +1, Charisma +2" })).toBeTruthy();
    expect(within(pane).getByText("Intelligence")).toBeTruthy();
    expect(within(pane).getByText("8 → 9")).toBeTruthy();
    expect(within(pane).getByText("8 → 10")).toBeTruthy();
    expect(within(pane).getByText("−1 → +0")).toBeTruthy();
  });

  it("desktop keeps the classic pane (the background), as Aurora shows no description for list entries", async () => {
    const user = userEvent.setup();
    const api = new ListSlotApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/builder/background", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: /^Intelligence \+1, Charisma \+2/ }));
    const pane = screen.getByRole("complementary", { name: "Description" });
    expect(within(pane).queryByRole("heading", { name: "Intelligence +1, Charisma +2" })).toBeNull();
  });
});
