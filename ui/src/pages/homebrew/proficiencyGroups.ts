import type { HomebrewRef } from "../../api/contract";

/** The class form's id and its proficiency fields (the descriptor's keys, `borealis-authoring` `kinds/class.rs`). */
export const CLASS_KIND = "class";
export const SAVES_FIELD = "savingThrows";
/** The reference fields a class lists its first-class proficiencies in, by kind. */
export const GROUP_FIELDS: readonly string[] = ["armor", "weapons", "tools"];

export type ProficiencyGroup = "armor" | "weapons" | "tools" | typeof SAVES_FIELD;

const SAVE_PREFIX = "ID_PROFICIENCY_SAVINGTHROW_";

/**
 * Where a class lists a proficiency, as the backend reads it back (`codec/class.rs` `in_group`): armor and weapon
 * proficiencies by their id prefixes, saving throws as the ability's chip, everything else (tools, homebrew) under Tool
 * and other proficiencies.
 */
export function proficiencyGroupOf(id: string): ProficiencyGroup {
  if (id.startsWith(SAVE_PREFIX)) return SAVES_FIELD;
  if (id.startsWith("ID_PROFICIENCY_ARMOR_")) return "armor";
  if (id.startsWith("ID_PROFICIENCY_WEAPON_")) return "weapons";
  return "tools";
}

/** "Strength" for `ID_PROFICIENCY_SAVINGTHROW_STRENGTH`. */
const abilityOf = (id: string): string => {
  const word = id.slice(SAVE_PREFIX.length).toLowerCase();
  return word.charAt(0).toUpperCase() + word.slice(1);
};

const refs = (v: unknown): HomebrewRef[] => (Array.isArray(v) ? v.filter((r): r is HomebrewRef => typeof r?.id === "string") : []);

export interface MovedProficiency {
  readonly name: string;
  readonly to: ProficiencyGroup;
}

/**
 * A class form's fields with proficiency group `key` set to `next`, and anything picked there that the class lists
 * under another kind moved to that kind's field (P3 Hpbig): what the editor shows is what a reopened class shows.
 */
export function withClassProficiencies(
  fields: Readonly<Record<string, unknown>>, key: string, next: unknown,
): { fields: Record<string, unknown>; moved: MovedProficiency[] } {
  const picked = refs(next);
  const stays = picked.filter((r) => proficiencyGroupOf(r.id) === key);
  const leaving = picked.filter((r) => proficiencyGroupOf(r.id) !== key);
  if (!leaving.length) return { fields: { ...fields, [key]: next }, moved: [] };
  const out: Record<string, unknown> = { ...fields, [key]: stays };
  for (const r of leaving) {
    const to = proficiencyGroupOf(r.id);
    if (to === SAVES_FIELD) {
      const saves = Array.isArray(out[SAVES_FIELD]) ? (out[SAVES_FIELD] as string[]) : [];
      const ability = abilityOf(r.id);
      out[SAVES_FIELD] = saves.includes(ability) ? saves : [...saves, ability];
    } else {
      const held = refs(out[to]);
      out[to] = held.some((h) => h.id === r.id) ? held : [...held, r];
    }
  }
  return { fields: out, moved: leaving.map((r) => ({ name: r.name, to: proficiencyGroupOf(r.id) })) };
}
