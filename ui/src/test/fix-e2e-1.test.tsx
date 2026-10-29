import { ABILITY_KEYS } from "../api/contract";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { STANDARD_ARRAY, applyStandardArray, type NewCharacterForm } from "../pages/characters/NewCharacterDialog";

const form: NewCharacterForm = {
  name: "Tess", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "standard_array",
  multiclassing: true, feats: true, averageHp: true, custom: {}, standardArray: STANDARD_ARRAY,
};

describe("standard array on create", () => {
  it("applies the default arrangement (15/14/13/12/10/8 to STR..CHA)", async () => {
    const api = new MockApi({ rng: () => 0.5 });
    const snap = await api.createCharacter({ request: { name: form.name, ruleset: form.ruleset, startingLevel: 1, gender: "", abilityMethod: "standard_array", options: {} } });
    const out = await applyStandardArray(api, snap, form);
    expect(ABILITY_KEYS.map((k) => out.abilities.base[k])).toEqual([15, 14, 13, 12, 10, 8]);
  });
});

describe("settings defaults", () => {
  it("match the settings spec", async () => {
    const s = await new MockApi().getSettings();
    expect(s).toMatchObject({ playerName: "Player One", abilityMethod: "roll_4d6", selectionFlow: false });
  });
});
