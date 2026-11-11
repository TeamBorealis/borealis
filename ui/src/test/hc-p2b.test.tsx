import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { HomebrewDraftView } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { renderApp } from "./utils";

// Homebrew P2b (1.5.0) on the web mock: a weapon's live preview gives every weapon fact, a creature's is the whole stat
// block.

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
});
