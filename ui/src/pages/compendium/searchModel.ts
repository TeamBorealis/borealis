import type { ElementSummary, Ruleset, SearchHit, SearchIndexStatus, SemanticIndexing, SemanticStatus, SearchScope, SearchSettings } from "../../api/contract";

/** The scopes the flyout offers (Sessions stays hidden until campaigns exist, spec section 11). */
export const SCOPES: { id: Exclude<SearchScope, "sessions">; label: string }[] = [
  { id: "all", label: "All" },
  { id: "rules", label: "Rules" },
  { id: "lore", label: "Lore" },
];

/** Group label of rules-text sections. */
export const RULES_GROUP = "Rules";

/** One result row, from a keyword hit or (empty query) a compendium listing. */
export interface ResultRow {
  key: string;
  /** What opens on select: an element id, or a `rules:` doc key. */
  open: string;
  name: string;
  /** Heading path of a rules section, shown before the name's source. */
  path: string;
  type: string;
  ruleset: Ruleset | null;
  source: string;
  title: string;
  snippet: string;
  highlights: [number, number][];
}

export function rowOfHit(h: SearchHit): ResultRow {
  return {
    key: h.docKey, open: h.elementId ?? h.docKey, name: h.title, path: h.type === "rules" ? h.path : "", type: h.type,
    ruleset: h.ruleset, source: h.source, title: h.snippet, snippet: h.snippet, highlights: h.highlights,
  };
}

export function rowOfElement(e: ElementSummary): ResultRow {
  return {
    key: e.id, open: e.id, name: e.name, path: "", type: e.type, ruleset: e.rulesets.length === 1 ? e.rulesets[0]! : null,
    source: e.sourceName, title: e.shortDescription, snippet: "", highlights: [],
  };
}

/** Rows grouped by type, groups in the order their first row appears (best hit's group first). */
export function groupRows(rows: ResultRow[]): [string, ResultRow[]][] {
  const groups = new Map<string, ResultRow[]>();
  for (const r of rows) groups.set(r.type, [...(groups.get(r.type) ?? []), r]);
  return [...groups.entries()];
}

/** Plain and highlighted pieces of a snippet (ranges may overlap or arrive unsorted). */
export function highlightParts(text: string, ranges: [number, number][]): { text: string; hit: boolean }[] {
  const sorted = [...ranges].filter(([s, e]) => s < e && s >= 0 && e <= text.length).sort((a, b) => a[0] - b[0]);
  const parts: { text: string; hit: boolean }[] = [];
  let at = 0;
  for (const [s, e] of sorted) {
    if (e <= at) continue;
    const start = Math.max(s, at);
    if (start > at) parts.push({ text: text.slice(at, start), hit: false });
    parts.push({ text: text.slice(start, e), hit: true });
    at = e;
  }
  if (at < text.length) parts.push({ text: text.slice(at), hit: false });
  return parts;
}

/** The edition toggle's starting value from `settings.search.defaultEdition`. */
export function initialEdition(setting: SearchSettings["defaultEdition"] | undefined, character: Ruleset | null): Ruleset | "any" {
  if (setting === "2014") return "dnd2014";
  if (setting === "2024") return "dnd2024";
  if (setting === "both") return "any";
  return character ?? "any";
}

/** R6 rule for search rows: the edition badge marks rows of the other edition than the one in play. */
export function badgeOf(row: ResultRow, preferred: Ruleset): "2014" | "2024" | null {
  if (!row.ruleset || row.ruleset === preferred) return null;
  return row.ruleset === "dnd2024" ? "2024" : "2014";
}

/** Footer line: the mode ("Keyword + semantic" only while embeddings really answer), plus keyword index progress, then
 * semantic index progress (spec section 11). */
export function footerText(status: SearchIndexStatus | null, semanticActive = false, indexing: SemanticIndexing | null = null): string {
  const mode = semanticActive ? "Keyword + semantic" : "Keyword search";
  if (status?.state === "building") return `${mode} - indexing ${status.done}/${status.total}`;
  if (status?.state === "failed") return `${mode} - index unavailable`;
  switch (indexing?.state) {
    case "loading": return `${mode} - loading the semantic model`;
    case "indexing": return `${mode} - semantic indexing ${indexing.total > 0 ? Math.floor((100 * indexing.done) / indexing.total) : 0}%`;
    case "paused": return `${mode} - semantic indexing paused`;
    case "disabled":
    case "failed": return `${mode} - semantic search unavailable`;
    default: return mode;
  }
}

/** The one-line hint under results (spec section 8): only where semantic search is off but can really be turned on
 * (a file this device can download or already has, and a linked embedding backend). Never a first-run prompt. */
export function showSemanticHint(s: SemanticStatus | null): boolean {
  return !!s && !s.semantic && !!s.file && (s.available || s.modelPresent) && s.backendAvailable;
}
