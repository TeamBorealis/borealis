import type { FiveToolsSource, ImportNote } from "../../api/contract";

/** Source list groups, in display order. Homebrew files and sources in no catalog share one group. */
export const SOURCE_GROUPS = [
  { id: "book", title: "Books" },
  { id: "adventure", title: "Adventures" },
  { id: "other", title: "Homebrew & other" },
] as const;

export type SourceGroupId = (typeof SOURCE_GROUPS)[number]["id"];
export type EditionFilter = "all" | "dnd2014" | "dnd2024";

export const groupOf = (s: FiveToolsSource): SourceGroupId =>
  s.category === "book" || s.category === "adventure" ? s.category : "other";

/** Mixed sources have entries of both editions, so they match either filter. */
export const matchesEdition = (s: FiveToolsSource, edition: EditionFilter): boolean =>
  edition === "all" || s.ruleset === edition || s.ruleset === "mixed";

export const matchesQuery = (s: FiveToolsSource, query: string): boolean => {
  const q = query.trim().toLowerCase();
  return !q || s.name.toLowerCase().includes(q) || s.abbr.toLowerCase().includes(q);
};

export const editionLabel = (r: FiveToolsSource["ruleset"]): string =>
  r === "dnd2024" ? "2024" : r === "mixed" ? "Mixed" : "2014";

/** Visible sources of each non-empty group. */
export function visibleGroups(sources: FiveToolsSource[], edition: EditionFilter, query: string) {
  return SOURCE_GROUPS.map((g) => ({
    ...g,
    sources: sources.filter((s) => groupOf(s) === g.id && matchesEdition(s, edition) && matchesQuery(s, query)),
  })).filter((g) => g.sources.length > 0);
}

/** `picked` with every source of `abbrs` turned on or off (order kept, no duplicates). */
export function setPicked(picked: string[], abbrs: string[], on: boolean): string[] {
  if (!on) return picked.filter((a) => !abbrs.includes(a));
  return [...picked, ...abbrs.filter((a) => !picked.includes(a))];
}

/** Notes that differ only in the quoted name or a number share a reason. */
export const reasonKey = (reason: string): string =>
  reason.replace(/"[^"]*"/g, "\"…\"").replace(/\d+/g, "N");

export interface NoteGroup { key: string; label: string; notes: ImportNote[] }

/** Notes grouped by reason, largest group first. */
export function groupNotes(notes: ImportNote[]): NoteGroup[] {
  const map = new Map<string, ImportNote[]>();
  for (const n of notes) {
    const k = reasonKey(n.reason);
    map.set(k, [...(map.get(k) ?? []), n]);
  }
  return [...map.entries()]
    .map(([key, list]) => ({ key, label: list.length === 1 ? list[0]!.reason : key, notes: list }))
    .sort((a, b) => b.notes.length - a.notes.length || a.label.localeCompare(b.label));
}

/** Skips that are expected: entries the core rules already define (languages, skills, item types...). */
export const isExpectedSkip = (n: ImportNote): boolean =>
  /core pack/.test(n.reason) || (n.kind === "language" && /already imported/.test(n.reason));
