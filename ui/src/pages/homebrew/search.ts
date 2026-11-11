import type { HomebrewElementSummary, HomebrewKindDescriptor } from "../../api/contract";

/** The element list filter (spec 12.3): typed words and the "has problems" switch. */
export interface ListFilter {
  text: string;
  problemsOnly: boolean;
}

export const EMPTY_FILTER: ListFilter = { text: "", problemsOnly: false };

/** A filter is searching: rows of every kind are shown, each with its kind. */
export const isSearching = (f: ListFilter): boolean => f.text.trim() !== "" || f.problemsOnly;

const hasProblems = (r: HomebrewElementSummary): boolean => r.problems.errors + r.problems.warnings > 0;

/** What one row is found by: its name, its kind (both labels), its element type, its tags and its text. */
function haystack(r: HomebrewElementSummary, kinds: readonly HomebrewKindDescriptor[]): string {
  const k = kinds.find((x) => x.id === r.kind);
  return [r.name, k?.label, k?.pluralLabel, r.elementType, ...(r.tags ?? []), r.text ?? ""].filter(Boolean).join(" \u0000 ").toLowerCase();
}

/**
 * The rows the list shows. Without a filter: the rows of the chosen kind. With one: rows of every kind in which every
 * typed word is found (name, kind, tag or text, any case), and only those with problems when that switch is on.
 */
export function filterRows(rows: readonly HomebrewElementSummary[], kindId: string | null, filter: ListFilter, kinds: readonly HomebrewKindDescriptor[]): HomebrewElementSummary[] {
  if (!isSearching(filter)) return rows.filter((r) => r.kind === kindId);
  const words = filter.text.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return rows.filter((r) => (!filter.problemsOnly || hasProblems(r)) && words.every((w) => haystack(r, kinds).includes(w)));
}

/**
 * The selection after a click (spec 12.5, Ctrl/Shift+click): a plain click chooses the row alone, Ctrl (Cmd) adds or
 * removes it, Shift adds the run from the last clicked row (`anchor`) in the order shown.
 */
export function nextSelection(current: ReadonlySet<string>, shown: readonly string[], id: string, anchor: string | null, mods: { toggle: boolean; range: boolean }): Set<string> {
  if (mods.range && anchor && shown.includes(anchor)) {
    const from = Math.min(shown.indexOf(anchor), shown.indexOf(id));
    const to = Math.max(shown.indexOf(anchor), shown.indexOf(id));
    return new Set([...current, ...shown.slice(from, to + 1)]);
  }
  if (mods.toggle) {
    const next = new Set(current);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return next;
  }
  return new Set([id]);
}
