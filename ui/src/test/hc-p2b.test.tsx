import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewDraftView } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { renderApp } from "./utils";

// Homebrew P2b (1.5.0) on the web mock: a weapon's live preview gives every weapon fact, a creature's is the whole stat
// block; a weapon tried out is carried, equipped (not attuned) and an attack, never a feature, and Weapon Mastery offers it.

const pane = () => screen.getByRole("complementary", { name: "Description" });

async function draftWith(api: MockApi, kind: string, fields: Record<string, unknown>): Promise<{ packId: string; draft: HomebrewDraftView }> {
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
  const fresh = await api.homebrewNewDraft({ packId: pack.id, kind });
  const draft = await api.homebrewUpdateDraft({ draftId: fresh.draftId, form: { ...fresh.form, fields: { ...fresh.form.fields, ...fields } } });
  return { packId: pack.id, draft };
}

const TWINBLADE = {
  name: "Twinblade",
  description: { type: "doc", content: [{ type: "paragraph", content: [{ type: "text", text: "Two curved blades share one long grip." }] }] },
  weaponCategory: { id: "ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE", name: "Martial Melee", type: "Weapon Category" },
  damage: "2d4",
  damageType: { id: "ID_INTERNAL_DAMAGE_TYPE_SLASHING", name: "Slashing", type: "Damage Type" },
  properties: [
    { id: "ID_INTERNAL_WEAPON_PROPERTY_TWOHANDED", name: "Two-Handed", type: "Weapon Property" },
    { id: "ID_INTERNAL_WEAPON_PROPERTY_FINESSE", name: "Finesse", type: "Weapon Property" },
  ],
  mastery: [{ id: "ID_2024_MASTERY_VEX", name: "Vex", type: "Mastery Property" }],
  cost: "50",
  currency: "gp",
  weight: "6",
};

describe("P2b creator previews (mock)", () => {
  it("a weapon's live preview gives its category, damage and type, properties, mastery, cost with its coin and weight", async () => {
    const api = new MockApi({ creatorEnabled: true });
    const { packId, draft } = await draftWith(api, "weapon", TWINBLADE);
    renderApp(`/homebrew/${packId}?draft=${draft.draftId}`, { api });
    await waitFor(() => expect(within(pane()).getByRole("heading", { level: 2, name: "Twinblade" })).toBeTruthy());
    const facts = [...pane().querySelectorAll(".desc-facts .desc-line")].map((p) => p.textContent);
    expect(facts).toEqual([
      "Weapon category. Martial Melee",
      "Cost. 50 gp",
      "Damage. 2d4 slashing",
      "Properties. Two-Handed, Finesse",
      "Mastery. Vex",
      "Weight. 6 lb.",
    ]);
    // the facts are a block of their own after the description, not its last line
    const description = pane().querySelector(".rich");
    expect(description?.textContent).toContain("Two curved blades");
    expect(description?.nextElementSibling?.classList.contains("desc-facts")).toBe(true);
  });

  it("a creature's live preview is the whole stat block, in its order", async () => {
    const api = new MockApi({ creatorEnabled: true });
    const { packId, draft } = await draftWith(api, "creature", {
      name: "Cinder Boar", size: ["medium"], creatureType: "beast", alignment: "unaligned",
      armorClass: { entries: [{ value: 11, source: "natural armor" }], note: "" }, hitPoints: { average: 11, formula: "2d8+2", note: "" },
      speed: { walk: 40 }, abilities: { str: 13, dex: 11, con: 12, int: 2, wis: 9, cha: 5 }, saves: { con: 3 }, skills: { perception: 1 },
      damageResistances: [{ types: ["fire"] }], conditionImmunities: ["frightened"], senses: { darkvision: 30, passive_perception: null },
      languages: [], challenge: { cr: "1/4", note: "", xp: null, xpLair: null, proficiencyBonus: null },
      actions: [{ name: "Tusk", text: "Melee Attack Roll: +3, reach 5 ft. Hit: 4 (1d6 + 1) Piercing damage." }],
    });
    renderApp(`/homebrew/${packId}?draft=${draft.draftId}`, { api });
    await waitFor(() => expect(within(pane()).getByRole("heading", { level: 2, name: "Cinder Boar" })).toBeTruthy());
    const text = pane().textContent ?? "";
    const order = ["Medium beast, unaligned", "Armor Class.", "11 (natural armor)", "Hit Points.", "11 (2d8+2)", "Speed.", "40 ft.",
      "STR", "CHA", "13 (+1)", "5 (-3)", "Saving Throws.", "Con +3", "Skills.", "Perception +1", "Damage Resistances.", "Fire",
      "Condition Immunities.", "Frightened", "Senses.", "Darkvision 30 ft., passive Perception 11", "Languages.", "—", "Challenge.",
      "1/4 (50 XP)", "Actions", "Tusk"];
    const at = order.map((s) => text.indexOf(s));
    expect(at.every((i) => i >= 0), order.filter((_, i) => at[i]! < 0).join(", ")).toBe(true);
    expect([...at].sort((a, b) => a - b)).toEqual(at);
    expect(within(pane()).getByRole("columnheader", { name: "WIS" })).toBeTruthy();
  });

  it("Try it: a weapon is carried and equipped, an attack, never a feature, and Weapon Mastery offers it", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const { packId, draft } = await draftWith(api, "weapon", TWINBLADE);
    renderApp(`/homebrew/${packId}?draft=${draft.draftId}`, { api });
    const editor = await screen.findByRole("region", { name: "Element editor" });
    await waitFor(() => expect((within(editor).getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe("Twinblade"));
    await user.click(within(editor).getByRole("button", { name: "Try it" }));
    const dialog = await screen.findByRole("dialog", { name: "Try it" });
    await within(dialog).findByText(/Your test character is level 1/);
    expect(within(dialog).getByText("Added to the inventory and equipped.")).toBeTruthy();
    expect(within(dialog).queryByText(/attuned/)).toBeNull();
    const attacks = within(dialog).getByRole("region", { name: "Attacks" });
    expect(within(attacks).getByText("Twinblade")).toBeTruthy();
    expect(attacks.textContent).toContain("2d4");
    expect(attacks.textContent).toContain("slashing");
    const features = within(dialog).getByRole("region", { name: "Features" });
    expect(within(features).queryByText("Twinblade")).toBeNull();
    const [mastery] = await within(dialog).findAllByRole("list", { name: "Weapon Mastery options" });
    if (!mastery) throw new Error("no Weapon Mastery picker");
    expect(await within(mastery).findByRole("button", { name: /^Twinblade/ })).toBeTruthy();
  });

  it("Edit on a loaded element in WHERE USED edits a deep copy of it in this pack (HC5)", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
    const newDraft = vi.spyOn(api, "homebrewNewDraft");
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "Spell Lists" }));
    const view = await screen.findByRole("region", { name: "Spell lists" });
    await user.selectOptions(within(view).getByRole("combobox", { name: "Spell list" }), "Wizard");
    const grid = await within(view).findByRole("table", { name: "Spells" });
    await user.click(within(grid).getByRole("button", { name: "Fireball" }));
    const where = await screen.findByRole("region", { name: "Where used" });
    await within(where).findByRole("button", { name: "Wizard" });
    await user.click(within(where).getByRole("button", { name: "Edit Wizard" }));
    await waitFor(() => expect(newDraft).toHaveBeenCalledWith({ packId: pack.id, kind: "other", copyFrom: "wizard", deepCopy: true }));
    const editor = await screen.findByRole("region", { name: "Element editor" });
    await waitFor(() => expect((within(editor).getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe("Wizard (copy)"));
  });

  it("SPELL LISTS: a subclass's expanded list is a list choice; checks are saved with its subclass", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
    const set = vi.spyOn(api, "homebrewSetSpellList");
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "Spell Lists" }));
    const view = await screen.findByRole("region", { name: "Spell lists" });
    const chooser = within(view).getByRole("combobox", { name: "Spell list" });
    await waitFor(() => expect(within(chooser).getByRole("option", { name: "Life Domain (expanded list)" })).toBeTruthy());
    await user.selectOptions(chooser, "expanded:life-domain");
    const grid = await within(view).findByRole("table", { name: "Spells" });
    expect(within(view).getByText(/Life Domain expanded list belongs to another pack's subclass/)).toBeTruthy();
    await user.click(within(grid).getByRole("checkbox", { name: "Choose Bless" }));
    await user.click(within(view).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(set).toHaveBeenCalledWith({ packId: pack.id, listTag: "", expandedOf: "life-domain", add: ["bless"], remove: [] }));
    await waitFor(() => expect(within(view).getByRole("status").textContent).toBe("1 spell on this list"));
    expect((within(grid).getByRole("checkbox", { name: "Choose Bless" }) as HTMLInputElement).checked).toBe(true);
  });
});
