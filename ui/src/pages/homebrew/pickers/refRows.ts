import type { HomebrewPickerRow, HomebrewPickerTarget, HomebrewRef, SpellFacts } from "../../../api/contract";
import type { ShownRow, VersionLike } from "../../../lib/versions";
import type { SpellFilterFacts } from "../../spells/SpellFilter";

/** A picker row as the Build picker's rows hold an option (`PickerRow` reads it): the id travels, never shown. */
export type RefVersion = VersionLike & { row: HomebrewPickerRow };

/** One reference picker row in the Build picker's shape. The source name doubles as the source key (rows carry no id). */
export function toVersion(row: HomebrewPickerRow): RefVersion {
  return {
    // The creator's type names (`Spell`) are not the app's element types; the Build row never reads them.
    id: row.id, name: row.name, type: row.type as VersionLike["type"], sourceName: row.sourceName, source: row.sourceName,
    shortDescription: row.summary, prerequisites: row.prerequisites, rulesets: row.edition ? [row.edition] : [],
    ...(row.level !== undefined ? { level: row.level } : {}), ...(row.school ? { school: row.school } : {}), ...(row.spell ? { spell: row.spell } : {}),
    row,
  };
}

/** One shown row per element (the creator lists each element once; the edition mark tells same-name elements apart). */
export function shownRow(v: RefVersion, chosen: boolean): ShownRow<RefVersion> {
  // Ids are unique per type; the key holds both so two loaded packs (or the mock) reusing an id across types stay apart.
  return { key: `${v.type}\u0000${v.id}`, name: v.name, all: [v], versions: [v], shown: v, chosen: chosen ? v : null };
}

/** The spell grid's view of a row (Magic filter bar, spec 13.7). */
export const spellFactsOf = (r: HomebrewPickerRow): SpellFilterFacts => ({
  name: r.name, school: r.school ?? "", sourceName: r.sourceName, level: r.level ?? 0,
  ritual: !!r.spell?.ritual, concentration: !!r.spell?.concentration,
});

export const castingOf = (r: HomebrewPickerRow): SpellFacts | null => r.spell ?? null;

/** A spell picker shows the Magic grid instead of the Build list (spec 5.1: "Spell pickers add the Magic filter bar"). */
export const isSpellTarget = (t: HomebrewPickerTarget): boolean => t.types.length === 1 && t.types[0]!.toLowerCase() === "spell";

export const refOf = (r: HomebrewPickerRow): HomebrewRef => ({ id: r.id, name: r.name, type: r.type });

/** Ids compare case-insensitively (hc-wb02: `equipped` values are stored lower case). */
export const sameId = (a: string, b: string): boolean => a.toLowerCase() === b.toLowerCase();

/** "Spells", "Feats", "Elements": the noun a picker searches. */
export function targetNoun(t: HomebrewPickerTarget): string {
  if (t.types.length !== 1) return "elements";
  const one = t.types[0]!.toLowerCase();
  return one.endsWith("s") ? one : one.endsWith("y") ? `${one.slice(0, -1)}ies` : `${one}s`;
}
