import type { ElementSummary, Page, Ruleset, SearchHit, SearchIndexStatus, SearchPassage, SearchQuery } from "../types";
import { apiError } from "./errors";

/** A searchable mock element (name, type, source and plain text). */
export interface MockSearchDoc extends Pick<ElementSummary, "id" | "name" | "type" | "source" | "sourceName" | "rulesets"> {
  text: string;
}

/**
 * Invented rules-text sections (not SRD wording) standing in for the bundled rules-text packs, so the Rules scope,
 * the passage view and its attribution line can be exercised without a backend.
 */
const RULES: (SearchPassage & { words: string })[] = [
  {
    docKey: "rules:mock-rules-text/combat/dash", title: "Dash", path: "Combat > Actions > Dash", ruleset: "dnd2024", page: 12,
    html: "<p>Mock rules text: taking this action adds your speed again to the distance you may move this turn.</p>",
    words: "Mock rules text: taking this action adds your speed again to the distance you may move this turn.",
    source: "Mock Rules Text", license: "CC-BY-4.0", attribution: "Mock attribution: invented sample text for the browser preview.",
  },
  {
    docKey: "rules:mock-rules-text/combat/cover", title: "Cover", path: "Combat > Cover", ruleset: "dnd2024", page: 14,
    html: "<p>Mock rules text: a wall or a creature between you and an attacker makes you harder to hit.</p>",
    words: "Mock rules text: a wall or a creature between you and an attacker makes you harder to hit.",
    source: "Mock Rules Text", license: "CC-BY-4.0", attribution: "Mock attribution: invented sample text for the browser preview.",
  },
  {
    docKey: "rules:mock-rules-text-2014/spellcasting/areas", title: "Areas of Effect", path: "Spellcasting > Areas of Effect", ruleset: "dnd2014", page: 20,
    html: "<p>Mock rules text: some spells, such as a fireball, fill a sphere or a cone instead of targeting one creature.</p>",
    words: "Mock rules text: some spells, such as a fireball, fill a sphere or a cone instead of targeting one creature.",
    source: "Mock Rules Text 2014", license: "CC-BY-4.0", attribution: "Mock attribution: invented sample text for the browser preview.",
  },
];

const SNIPPET = 160;

/** Levenshtein distance, capped (small strings only). */
function distance(a: string, b: string): number {
  if (Math.abs(a.length - b.length) > 2) return 3;
  let prev = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    const row = [i];
    for (let j = 1; j <= b.length; j++) row.push(Math.min(prev[j]! + 1, row[j - 1]! + 1, prev[j - 1]! + (a[i - 1] === b[j - 1] ? 0 : 1)));
    prev = row;
  }
  return prev[b.length]!;
}

/** A crude stand-in for the Tantivy ranking: exact > prefix > word prefix > typo > substring > text match. */
function score(name: string, text: string, q: string): number {
  const n = name.toLowerCase();
  if (n === q) return 100;
  if (n.startsWith(q)) return 80;
  const words = n.split(/[^a-z0-9]+/).filter(Boolean);
  if (words.some((w) => w.startsWith(q))) return 60;
  if (q.length >= 4 && (distance(n, q) <= 2 || words.some((w) => distance(w, q) <= (q.length >= 8 ? 2 : 1)))) return 50;
  if (n.includes(q)) return 40;
  const terms = q.split(/\s+/).filter((t) => t.length > 1);
  const t = text.toLowerCase();
  const found = terms.filter((w) => t.includes(w)).length;
  return found > 0 && found === terms.length ? 10 + found : 0;
}

function snippetOf(text: string, q: string): { snippet: string; highlights: [number, number][] } {
  const terms = q.toLowerCase().split(/\s+/).filter((t) => t.length > 1);
  const lower = text.toLowerCase();
  const first = Math.min(...terms.map((t) => lower.indexOf(t)).filter((i) => i >= 0), Number.MAX_SAFE_INTEGER);
  const start = first === Number.MAX_SAFE_INTEGER ? 0 : Math.max(0, first - 40);
  const snippet = text.slice(start, start + SNIPPET);
  const highlights: [number, number][] = [];
  const sl = snippet.toLowerCase();
  for (const term of terms) {
    for (let i = sl.indexOf(term); i >= 0; i = sl.indexOf(term, i + term.length)) highlights.push([i, i + term.length]);
  }
  return { snippet, highlights: highlights.sort((a, b) => a[0] - b[0]) };
}

const editionOk = (rulesets: Ruleset[], edition: SearchQuery["edition"]) => !edition || edition === "any" || rulesets.includes(edition);

/** Mock `search`: Lore = the mock elements, Rules = the invented sections; `all` interleaves them by rank. */
export function mockSearch(docs: MockSearchDoc[], q: SearchQuery): Page<SearchHit> {
  const text = q.query.trim().toLowerCase();
  if (!text || q.scope === "sessions") return { total: 0, items: [] };
  const typeOk = (t: string) => !q.types?.length || (q.types as string[]).includes(t);
  const sourceOk = (s: string) => !q.sources?.length || q.sources.includes(s);
  const lore = docs
    .filter((d) => typeOk(d.type) && sourceOk(d.source) && editionOk(d.rulesets, q.edition) && !["multiclass", "asi", "list"].includes(d.type))
    .map((d) => ({ d, s: score(d.name, d.text, text) }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s || a.d.name.localeCompare(b.d.name))
    .map(({ d }): SearchHit => ({
      docKey: d.id, title: d.name, path: "", type: d.type, ruleset: d.rulesets.length === 1 ? d.rulesets[0]! : null, source: d.sourceName,
      ...snippetOf(d.text, text), elementId: d.id,
    }));
  const rules = RULES
    .filter((r) => typeOk("rules") && sourceOk(r.source) && editionOk([r.ruleset], q.edition))
    .map((r) => ({ r, s: score(r.title, `${r.path} ${r.words}`, text) }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s)
    .map(({ r }): SearchHit => ({ docKey: r.docKey, title: r.title, path: r.path, type: "rules", ruleset: r.ruleset, source: r.source, ...snippetOf(r.words, text), elementId: null }));
  const picked = q.scope === "rules" ? rules : q.scope === "lore" ? lore : interleave(lore, rules);
  return { total: picked.length, items: picked.slice(q.offset, q.offset + q.limit) };
}

function interleave(a: SearchHit[], b: SearchHit[]): SearchHit[] {
  const out: SearchHit[] = [];
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    if (a[i]) out.push(a[i]!);
    if (b[i]) out.push(b[i]!);
  }
  return out;
}

/** Mock `get_search_passage`. */
export function mockPassage(docKey: string): SearchPassage {
  const hit = RULES.find((r) => r.docKey === docKey.replace(/#\d+$/, ""));
  if (!hit) throw apiError("not_found", "That rules section does not exist.");
  const { words: _w, ...passage } = hit;
  return passage;
}

/** The mock index is always ready. */
export const MOCK_INDEX_STATUS: SearchIndexStatus = { state: "ready", done: 3, total: 3, message: null };
