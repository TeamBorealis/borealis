import { describe, expect, it } from "vitest";
import type { ClassSpellcasting, SpellcastingState } from "../api/contract";
import { magicSpellcasting } from "../pages/spells/magicSpellcasting";

/** Aurora's Spellcasting tab has no section for spells granted with no spellcasting name; only its sheet lists them. */
const block = (className: string, other?: boolean): ClassSpellcasting => ({
  classSlotId: "main", className, ability: "cha", saveDc: 13, attackBonus: 5, attackRoll: 5, model: "known", cantripsMax: 2,
  cantripsKnown: 2, preparedMax: 0, preparedCount: 0, maxSpellLevel: 1, pact: null, spells: [], ...(other ? { other } : {}),
});

describe("Magic page: Other Spells", () => {
  it("leaves the Other Spells block out and keeps the rest", () => {
    const sc: SpellcastingState = { classes: [block("Warlock"), block("Other Spells", true)], slots: [], multiclassSlots: null };
    expect(magicSpellcasting(sc).classes.map((c) => c.className)).toEqual(["Warlock"]);
    expect(sc.classes).toHaveLength(2);
  });
});
