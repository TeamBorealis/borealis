import type {
  ElementSummary, HomebrewBlock, HomebrewBlockSpec, HomebrewExprNode, HomebrewFieldSpec, HomebrewForm, HomebrewKindDescriptor, HomebrewPickerFilters, HomebrewPickerRow,
  HomebrewPickerTarget, HomebrewProblem, Page,
} from "../types";
import { blockShapeProblems } from "./homebrew-shapes";

// The web mock's stand-ins for what WB-06/WB-07 do on the backend: picker rows (`homebrew_reference_options`), the live
// problems of forms, blocks and conditions (the backend's `borealis_authoring::check` sentences, word for word), and the
// description preview. Enough for the creator's controls to be used and tested
// on the mock; the real answers come from `borealis-authoring`.

/** "Magic Item" -> "magic-item": the creator's type names against the mock's element types. */
const typeKey = (t: string): string => t.trim().toLowerCase().replace(/\s+/g, "-");

/** One row per word (the generic form's open types and setter names, WB-19), filtered by the query and paged. */
export function wordRows(words: readonly string[], type: string, query: string, page: { offset: number; limit: number }): Page<HomebrewPickerRow> {
  const q = query.trim().toLowerCase();
  const rows = words.filter((w) => w.toLowerCase().includes(q))
    .map((w): HomebrewPickerRow => ({ id: w, name: w, type, summary: "", prerequisites: null, sourceName: "", edition: null, draft: false }));
  return { total: rows.length, items: rows.slice(page.offset, page.offset + page.limit) };
}

/** An element's row as the reference picker shows it (spec 13.5). */
export function pickerRow(e: ElementSummary): HomebrewPickerRow {
  return {
    id: e.id, name: e.name, type: e.type, summary: e.shortDescription, prerequisites: e.prerequisites, sourceName: e.sourceName,
    edition: e.rulesets.length === 1 ? e.rulesets[0]! : null, draft: false,
    ...(e.level !== undefined ? { level: e.level } : {}), ...(e.school ? { school: e.school } : {}), ...(e.spell ? { spell: e.spell } : {}),
  };
}

/** The classes that declare a spell list on the mock: a loaded class whose name some loaded spell carries as a tag
 * (the backend reads each class's `spellcasting.list.supports`; `homebrew_refs::SPELL_LIST_TYPE`). */
function listClasses(loaded: readonly ElementSummary[]): ElementSummary[] {
  const tags = new Set(loaded.filter((e) => e.type === "spell").flatMap((e) => (e.tags ?? []).map((t) => t.toLowerCase())));
  return loaded.filter((e) => e.type === "class" && (tags.has(e.id.toLowerCase()) || tags.has(e.name.toLowerCase())));
}

/** The lists the bundled SRD caster classes declare, always loaded in the app (`homebrew_spell.rs`
 * `the_spell_lists_offered_are_the_lists_loaded_classes_declare`): the mock's real SRD copies carry them. */
const SRD_SPELL_LISTS = ["Bard", "Cleric", "Druid", "Paladin", "Ranger", "Sorcerer", "Warlock", "Wizard"];

/** The spell lists the spell list lints know on the mock: the SRD casters' and the mock's loaded classes', by name. */
export function spellListNames(loaded: readonly ElementSummary[]): string[] {
  return [...new Set([...SRD_SPELL_LISTS, ...listClasses(loaded).map((e) => e.name)])];
}

/** The Spell List picker rows (`homebrew_refs::SPELL_LIST_TYPE`). */
function spellListRows(loaded: readonly ElementSummary[], query: string, page: { offset: number; limit: number }): Page<HomebrewPickerRow> {
  const words = query.trim().toLowerCase();
  const rows = listClasses(loaded).filter((e) => e.name.toLowerCase().includes(words))
    .map((e): HomebrewPickerRow => ({ id: e.name, name: e.name, type: "Spell List", summary: "", prerequisites: null, sourceName: e.sourceName, edition: null, draft: false }))
    .sort((a, b) => a.name.localeCompare(b.name));
  return { total: rows.length, items: rows.slice(page.offset, page.offset + page.limit) };
}

/** The Levenshtein distance of two words (`check::lints::edit_distance`). */
function editDistance(a: string, b: string): number {
  let row = Array.from({ length: b.length + 1 }, (_, j) => j);
  for (let i = 0; i < a.length; i += 1) {
    const next = [i + 1];
    for (let j = 0; j < b.length; j += 1) next.push(Math.min(row[j]! + (a[i] === b[j] ? 0 : 1), next[j]! + 1, row[j + 1]! + 1));
    row = next;
  }
  return row[b.length]!;
}

/** A known list spelled like `list` but for case, or within two letters of it (`check::lints::near_list`). */
function nearList(list: string, known: readonly string[]): string | undefined {
  const fold = (t: string) => t.trim().toLowerCase();
  return known.find((k) => fold(k) === fold(list)) ?? (fold(list).length > 3 ? known.find((k) => editDistance(fold(k), fold(list)) <= 2) : undefined);
}

/** A spell's lists no loaded class declares (warnings) and a spell on no list (a hint), as `check::lints` words them. */
export function spellListProblems(form: HomebrewForm, elementId: string | null, known: readonly string[]): HomebrewProblem[] {
  if (form.kind !== "spell") return [];
  const path = "fields.spellLists";
  const lists = Array.isArray(form.fields.spellLists) ? form.fields.spellLists.map(String) : [];
  if (lists.length === 0) {
    return [{ severity: "info", path, elementId, code: "lint.no_spell_list", message: "This spell is on no spell list, so no class offers it. It can still be given by a feature." }];
  }
  return lists.filter((l) => !known.includes(l)).map((list): HomebrewProblem => {
    const near = nearList(list, known);
    const head = `No loaded class or choice uses the spell list "${list}".`;
    const tail = near === undefined ? "Check the spelling, or make the class that has it."
      : near.toLowerCase() === list.toLowerCase() ? `Lists match by exact spelling: there is a list "${near}".` : `Check the spelling: there is a list "${near}".`;
    return { severity: "warning", path, elementId, code: "lint.unknown_spell_list", message: `${head} ${tail}` };
  });
}

/** A picker candidate: its row, and the tags it carries (`null`: already checked, the mock's own rows). */
interface Candidate { row: HomebrewPickerRow; tags: readonly string[] | null }

/** The Magic filter bar's tests (`homebrew_refs::spell_filters_pass`): with any of them set, spells only. */
function spellFiltersPass(r: HomebrewPickerRow, f: HomebrewPickerFilters): boolean {
  const wantsSpell = !!f.spellSchool || (f.spellLevels?.length ?? 0) > 0 || f.ritual === true || f.concentration === true;
  if (!wantsSpell) return true;
  if (typeKey(r.type) !== "spell") return false;
  if (f.spellSchool && (r.school ?? "").trim().toLowerCase() !== f.spellSchool.toLowerCase()) return false;
  if (f.spellLevels?.length && (r.level === undefined || !f.spellLevels.includes(r.level))) return false;
  return (f.ritual !== true || r.spell?.ritual === true) && (f.concentration !== true || r.spell?.concentration === true);
}

/** Does a candidate pass, as the backend's `homebrew_refs::passes` decides (Hlw0Y): one of the target's types, one of its
 * tags exactly, the query (trimmed, any case) in the name, one of the books, the edition and the Magic filters. */
function passes(c: Candidate, target: HomebrewPickerTarget, types: ReadonlySet<string>, query: string, f: HomebrewPickerFilters): boolean {
  const r = c.row;
  // the core proficiency rows (WB-16) are offered to proficiency pickers only: their skill names are the sample skills'
  if (types.size === 0 ? r.id.startsWith("ID_PROFICIENCY_") : !types.has(typeKey(r.type))) return false;
  const tags = target.tags ?? [];
  if (c.tags !== null && tags.length > 0 && !tags.some((t) => c.tags!.includes(t))) return false;
  if (query && !r.name.toLowerCase().includes(query)) return false;
  if (f.sources?.length && !f.sources.includes(r.sourceName)) return false;
  if (f.edition && r.edition && r.edition !== f.edition) return false;
  return spellFiltersPass(r, f);
}

/** The backend's row order (`homebrew_refs::picker_order`): by name in any case, then book, then id. */
function pickerOrder(a: HomebrewPickerRow, b: HomebrewPickerRow): number {
  const cmp = (x: string, y: string) => (x < y ? -1 : x > y ? 1 : 0);
  return cmp(a.name.toLowerCase(), b.name.toLowerCase()) || cmp(a.sourceName, b.sourceName) || cmp(a.id, b.id);
}

/** Rows for a picker (`homebrew_reference_options`): the pack's own rows (filtered by tag already) and the loaded
 * elements, each id once, that pass the target, the query and the filters; one page of them. */
export function referenceRows(loaded: readonly ElementSummary[], own: readonly HomebrewPickerRow[], target: HomebrewPickerTarget, query: string,
  page: { offset: number; limit: number }, filters: HomebrewPickerFilters = {}): Page<HomebrewPickerRow> {
  if (target.types.includes("Spell List")) return spellListRows(loaded, query, page);
  const types = new Set(target.types.map(typeKey));
  const q = query.trim().toLowerCase();
  const seen = new Set<string>();
  const rows = [...own.map((row): Candidate => ({ row, tags: null })), ...loaded.map((e): Candidate => ({ row: pickerRow(e), tags: e.tags ?? [] }))]
    .filter((c) => passes(c, target, types, q, filters) && !seen.has(c.row.id) && !!seen.add(c.row.id))
    .map((c) => c.row)
    .sort(pickerOrder);
  return { total: rows.length, items: rows.slice(page.offset, page.offset + page.limit) };
}

const empty = (v: unknown): boolean => v === undefined || v === null || v === "" || (Array.isArray(v) && v.length === 0);

/** Condition rows the builder's check refuses (hc-wb02 `BuilderProblem`), with their model paths. */
function conditionProblems(node: HomebrewExprNode<Record<string, unknown>>, path: string): { path: string; message: string }[] {
  if (node.node === "group") return node.items.flatMap((n, i) => conditionProblems(n as HomebrewExprNode<Record<string, unknown>>, `${path ? `${path}.` : ""}items.${i}`));
  const row = node.row;
  if ((row.row === "has" || row.row === "lacks") && !row.id) return [{ path, message: "Pick the element this condition tests." }];
  if (row.row === "ability" && (Number(row.min) < 1 || Number(row.min) > 30)) return [{ path: `${path}.min`, message: "An ability score is between 1 and 30." }];
  if (row.row === "classLevel" && !row.class) return [{ path: `${path}.class`, message: "Choose the class whose level counts." }];
  if (row.row === "feature" && !String(row.name ?? "").trim()) return [{ path: `${path}.name`, message: "Give the feature's name." }];
  return [];
}

/** Problems of a builder value at `path` (`fields.requirements`, `blocks.0.fields.requirements`). */
function exprProblems(v: unknown, path: string, elementId: string | null): HomebrewProblem[] {
  const model = (v as { value?: { shape?: string; model?: unknown } } | null)?.value;
  if (!model || model.shape !== "model" || typeof model.model !== "object" || model.model === null || !("node" in model.model)) return [];
  return conditionProblems(model.model as HomebrewExprNode<Record<string, unknown>>, "").map((p) => ({
    severity: "error", message: p.message, path: p.path ? `${path}.${p.path}` : path, elementId, code: "builder",
  }));
}

/** A block's label as the start of a sentence ("Gives ..." -> "Gives"), as `check::fields` writes it. */
const blockLabel = (spec: HomebrewBlockSpec): string => spec.label.replace(/[.\s]+$/, "");

/** "From level" of gated blocks (`check::fields::LEVELS`). */
const LEVELS = { min: 1, max: 20 };

/** Dice text: terms joined by + or -, each a whole number or NdM (`check::is_dice`). */
export function isDice(text: string): boolean {
  const t = text.replace(/\s+/g, "").replace(/^[+-]/, "").toLowerCase();
  return t !== "" && t.split(/[+-]/).every((term) => /^(\d*d\d+|\d+)$/.test(term));
}

/** Numbers out of range and dice that aren't dice, in a block's own fields (the backend's `block.range` / `block.dice`). */
function valueProblems(spec: HomebrewBlockSpec, b: HomebrewBlock, i: number, elementId: string | null): HomebrewProblem[] {
  const label = blockLabel(spec);
  const at = (key: string) => `blocks.${i}.fields.${key}`;
  const out: HomebrewProblem[] = [];
  const range = (key: string, field: string, min: number, max: number): void => {
    out.push({ severity: "error", message: `${label}: ${field.toLowerCase()} must be a whole number from ${min} to ${max}.`, path: at(key), elementId, code: "block.range" });
  };
  const level = b.fields.level;
  if (spec.levelGated && typeof level === "number" && (!Number.isInteger(level) || level < LEVELS.min || level > LEVELS.max)) range("level", "From level", LEVELS.min, LEVELS.max);
  for (const f of spec.fields) {
    const v = b.fields[f.key];
    if (f.control === "number" && typeof v === "number" && f.min !== undefined && f.max !== undefined && (!Number.isInteger(v) || v < f.min || v > f.max)) range(f.key, f.label, f.min, f.max);
    if (f.control === "dice" && typeof v === "string" && v.trim() !== "" && !isDice(v)) {
      out.push({ severity: "warning", message: `${label}: write ${f.label.toLowerCase()} as dice, like 2d6 or 1d8+2.`, path: at(f.key), elementId, code: "block.dice" });
    }
  }
  return out;
}

/** A block's required fields, ranges, dice and its condition (the backend's `blocks.N.fields.<key>` sentences). */
function blockProblems(b: HomebrewBlock, i: number, catalog: readonly HomebrewBlockSpec[], elementId: string | null): HomebrewProblem[] {
  const spec = catalog.find((s) => s.kind === b.kind);
  if (!spec) return [];
  const missing = spec.fields.filter((f) => f.required && f.control !== "auto" && empty(b.fields[f.key])).map((f): HomebrewProblem => ({
    severity: "error", message: `${blockLabel(spec)}: fill in ${f.label.toLowerCase()}.`, path: `blocks.${i}.fields.${f.key}`, elementId, code: "block.required",
  }));
  return [...blockShapeProblems(b, i, catalog, elementId), ...missing, ...valueProblems(spec, b, i, elementId), ...exprProblems(b.fields.requirements, `blocks.${i}.fields.requirements`, elementId)];
}

/** "a deity", "an alignment", "domains" (`check::messages::with_article`). */
export function withArticle(noun: string): string {
  if (noun.endsWith("s") && !noun.endsWith("ss") && !noun.endsWith("us")) return noun;
  return `${/^[aeiou]/i.test(noun) ? "an" : "a"} ${noun}`;
}

/** Required fields of the form left empty: "Give it a name.", "A deity needs an alignment." (`check::fields`). */
function requiredProblems(form: HomebrewForm, kind: HomebrewKindDescriptor | undefined, elementId: string | null): HomebrewProblem[] {
  type Field = Pick<HomebrewFieldSpec, "key" | "label" | "required" | "readOnly" | "control">;
  const fields: readonly Field[] = kind?.fields ?? [{ key: "name", label: "Name", required: true, control: "text" }];
  const subject = kind?.id === "other" || !kind ? "This element" : withArticle(kind.label.toLowerCase()).replace(/^./, (c) => c.toUpperCase());
  const blank = (v: unknown): boolean => (typeof v === "string" ? v.trim() === "" : empty(v));
  return fields.filter((f) => f.required && !f.readOnly && f.control !== "reference" && blank(form.fields[f.key])).map((f): HomebrewProblem => ({
    severity: "error", path: `fields.${f.key}`, elementId, code: "form.required",
    message: f.key === "name" ? "Give it a name." : `${subject} needs ${withArticle(f.label.toLowerCase())}.`,
  }));
}

/** Statistics a skill can't be named like (the engine's built-in groups that are not skills, `content_skills::skill_key`). */
const NOT_SKILL_NAMES = ["strength", "dexterity", "constitution", "intelligence", "wisdom", "charisma", "hp", "level", "proficiency", "ac",
  "speed", "innate speed", "initiative", "ability checks"];

/** A Skill whose name is a statistic or holds a colon is no character skill (`check::fields`, `form.skill_name`). */
function skillNameProblems(form: HomebrewForm, elementId: string | null): HomebrewProblem[] {
  if (form.kind !== "skill") return [];
  const name = typeof form.fields.name === "string" ? form.fields.name.trim().split(/\s+/).join(" ").toLowerCase() : "";
  if (!name || (!name.includes(":") && !NOT_SKILL_NAMES.includes(name))) return [];
  return [{ severity: "error", path: "fields.name", elementId, code: "form.skill_name",
    message: "A skill can't be named like an ability or another statistic (Strength, AC, Level), or hold a colon: rename it." }];
}

/** Most skills a class's character chooses from (`codec/class.rs` `write_skills`). */
const MAX_SKILL_CHOICE = 18;

/** A class's skill choice, as the backend's class codec refuses it (`write_skills`, IDprw): pinned by
 * `__fixtures__/homebrew-class-skills.json` (`crates/borealis-authoring/tests/mock_class_skills.rs`). */
function classSkillProblems(form: HomebrewForm, elementId: string | null): HomebrewProblem[] {
  if (form.kind !== "class") return [];
  const v = form.fields.skills as { count?: unknown; skills?: unknown; tagged?: unknown } | undefined;
  if (!v || typeof v !== "object") return [];
  const count = Number(v.count ?? 0);
  const tagged = typeof v.tagged === "string" ? v.tagged.trim() : "";
  const ids = (Array.isArray(v.skills) ? v.skills : []).map((s) => String((s as { id?: unknown })?.id ?? "").trim()).filter(Boolean);
  const problem = (message: string): HomebrewProblem[] => [{ severity: "error", path: "fields.skills", elementId, code: "form", message }];
  if (!tagged && ids.length === 0) return count === 0 ? [] : problem("Pick the skills the character chooses from.");
  if (count === 0 || count > MAX_SKILL_CHOICE) return problem(`The character chooses from 1 to ${MAX_SKILL_CHOICE} skills.`);
  if (!tagged && count > ids.length) return problem("Offer at least as many skills as the character chooses.");
  return [];
}

/** The mock's live problems of a form: required fields, the prerequisite's rows, every block. The engine checks
 * (`homebrew_check_draft`: test character, level sweep, balance) are the backend's only; the mock has none. */
export function formProblems(form: HomebrewForm, catalog: readonly HomebrewBlockSpec[], elementId: string | null, kind?: HomebrewKindDescriptor): HomebrewProblem[] {
  return [
    ...requiredProblems(form, kind, elementId),
    ...skillNameProblems(form, elementId),
    ...classSkillProblems(form, elementId),
    ...exprProblems(form.fields.requirements, "fields.requirements", elementId),
    ...form.blocks.flatMap((b, i) => blockProblems(b, i, catalog, elementId)),
  ];
}

interface PmNode { type: string; text?: string; marks?: { type: string; attrs?: { tag?: string } }[]; content?: PmNode[]; attrs?: Record<string, unknown> }

const esc = (t: string): string => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

function inline(nodes: readonly PmNode[] = []): string {
  return nodes.map((n) => {
    if (n.type === "hard_break") return "<br />";
    let html = esc(n.text ?? "");
    for (const m of [...(n.marks ?? [])].reverse()) html = `<${m.attrs?.tag ?? (m.type === "bold" ? "b" : "i")}>${html}</${m.attrs?.tag ?? (m.type === "bold" ? "b" : "i")}>`;
    return html;
  }).join("");
}

function block(n: PmNode): string {
  switch (n.type) {
    case "paragraph": return `<p>${inline(n.content)}</p>`;
    case "heading": return `<h${n.attrs?.level === 4 ? 4 : 3}>${inline(n.content)}</h${n.attrs?.level === 4 ? 4 : 3}>`;
    case "bullet_list": case "ordered_list": {
      const tag = n.type === "bullet_list" ? "ul" : "ol";
      return `<${tag}>${(n.content ?? []).map((li) => `<li>${inline(li.content)}</li>`).join("")}</${tag}>`;
    }
    case "table": return `<table>${(n.content ?? []).map((tr) => `<tr>${(tr.content ?? []).map((c) => (c.type === "table_header" ? `<th>${inline(c.content)}</th>` : `<td>${inline(c.content)}</td>`)).join("")}</tr>`).join("")}</table>`;
    case "blockquote": return `<blockquote>${(n.content ?? []).map(block).join("")}</blockquote>`;
    case "kept": return String(n.attrs?.html ?? "");
    default: return "";
  }
}

/** The description preview: the mock stands in for the backend's one HTML writer (spec 7.2); plain text is a paragraph. */
export function descriptionHtml(v: unknown): string {
  if (typeof v === "string") return v.trim() ? `<p>${esc(v.trim())}</p>` : "";
  const doc = v as PmNode | null;
  return doc && doc.type === "doc" ? (doc.content ?? []).map(block).join("\n") : "";
}
