import type { SpellcastingState } from "../../api/contract";

/**
 * The spellcasting the Magic page shows: every block but "Other Spells" (spells granted with no spellcasting name, an
 * invocation's or a race's). Aurora's Spellcasting tab has no section for them either; they print on the sheet's
 * "Other Spells" section and are listed in Play.
 */
export function magicSpellcasting(sc: SpellcastingState): SpellcastingState {
  return { ...sc, classes: sc.classes.filter((c) => !c.other) };
}
