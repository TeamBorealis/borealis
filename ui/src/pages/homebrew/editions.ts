import type { HomebrewFieldSpec, Ruleset } from "../../api/contract";

/**
 * Fields of one edition (`HomebrewFieldSpec.editions`, WB-14/WB-16): a form of the other edition hides them, unless the
 * field still holds a value (a 2024 background read with a 2014 fixed language, a 2014 class with an epic boon). Such a
 * field still writes its rules, so it is shown with a note instead of holding rules nobody sees (P2a).
 */

/** The field is limited to editions other than `edition` (no edition = the pack's both: every field belongs). */
export const otherEdition = (f: HomebrewFieldSpec, edition: string | null): boolean =>
  !!f.editions && f.editions.length > 0 && !!edition && !f.editions.includes(edition as Ruleset);

/** Any text in a rich-text document. */
function docHasText(node: unknown): boolean {
  if (typeof node !== "object" || node === null) return false;
  const n = node as { text?: unknown; content?: unknown };
  if (typeof n.text === "string" && n.text.trim() !== "") return true;
  return Array.isArray(n.content) && n.content.some(docHasText);
}

/** A value that writes something: not empty, not 0 or false, not the field's starting value. */
function filled(v: unknown): boolean {
  if (v === null || v === undefined || v === false) return false;
  if (v === true) return true;
  if (typeof v === "number") return v !== 0;
  if (typeof v === "string") return v.trim() !== "";
  if (Array.isArray(v)) return v.length > 0;
  if (typeof v !== "object") return false;
  const o = v as Record<string, unknown>;
  if (o.type === "doc") return docHasText(o);
  // a written list holds its entries (its count alone writes nothing)
  if (Array.isArray(o.items)) return o.items.length > 0;
  return Object.values(o).some(filled);
}

/** The field's value writes something (it is not empty nor the field's starting value). */
export function holdsValue(f: HomebrewFieldSpec, v: unknown): boolean {
  if (f.default !== undefined && JSON.stringify(v) === JSON.stringify(f.default)) return false;
  return filled(v);
}

const YEAR: Record<string, string> = { dnd2014: "2014", dnd2024: "2024" };

/** The note under a field of the other edition that is shown because it holds a value. */
export function editionNote(f: HomebrewFieldSpec): string {
  const years = (f.editions ?? []).map((e) => YEAR[e] ?? e).join(" and ");
  return `This part belongs to the ${years} edition. It is shown because it still gives its rules: clear it to remove them.`;
}

/** The field is shown in a form of `edition`: its own edition's, or another's that holds a value. */
export const shownForEdition = (f: HomebrewFieldSpec, edition: string | null, v: unknown): boolean =>
  !otherEdition(f, edition) || holdsValue(f, v);
