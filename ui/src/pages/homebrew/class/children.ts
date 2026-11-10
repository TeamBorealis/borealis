import type { HomebrewChildEntry, HomebrewForm, HomebrewProblem, HomebrewTableColumn } from "../../../api/contract";

/**
 * Children of a form (spec 3.1, WB-16): the class's features and package, a feat's sub-features. They are child forms of
 * the draft (`form.children`), given by the parent's children field (`HomebrewChildEntry[]`): the LEVELS grid holds each
 * feature at a level. These helpers are pure: every edit makes a new form (spec 10.2, one undo step each).
 */

/** Levels of a class (spec 4.5: 20 rows). */
export const LEVELS = Array.from({ length: 20 }, (_, i) => i + 1);

/** The proficiency bonus at a character level (2 at 1-4 ... 6 at 17-20). */
export const proficiencyBonus = (level: number): number => 2 + Math.floor((level - 1) / 4);

/** The entries of a children field (none for a value of another shape). */
export function entriesOf(form: HomebrewForm, key: string): HomebrewChildEntry[] {
  const v = form.fields[key];
  return Array.isArray(v) ? (v as HomebrewChildEntry[]).filter((e) => typeof e === "object" && e !== null) : [];
}

/** The child form an entry points at. */
export const childOf = (form: HomebrewForm, entry: HomebrewChildEntry): HomebrewForm | undefined =>
  entry.child === undefined ? undefined : form.children.find((c) => c.key === entry.child);

/** A form's name as typed, else "New <kind>". */
export function nameOf(form: HomebrewForm | undefined, fallback = "New feature"): string {
  const n = form?.fields.name;
  return typeof n === "string" && n.trim() ? n.trim() : fallback;
}

/** What an entry shows: its child's name, or the element's ("Missing: <name>" when not loaded). */
export function entryName(form: HomebrewForm, entry: HomebrewChildEntry): string {
  const c = childOf(form, entry);
  if (c) return nameOf(c);
  if (entry.element) return entry.element.missing ? `Missing: ${entry.element.name || "a feature"}` : entry.element.name || "A feature";
  return "A feature";
}

export const withField = (form: HomebrewForm, key: string, value: unknown): HomebrewForm => ({ ...form, fields: { ...form.fields, [key]: value } });

/** The form with the child `key` replaced. */
export const withChild = (form: HomebrewForm, key: string, next: HomebrewForm): HomebrewForm =>
  ({ ...form, children: form.children.map((c) => (c.key === key ? next : c)) });

/** The level the children field `field` gives the child `key` at. */
export const levelOfChild = (form: HomebrewForm, field: string, key: string): number | undefined =>
  entriesOf(form, field).find((e) => e.child === key)?.level;

/** The child `key` moved to `level` on the grid. */
export function withChildLevel(form: HomebrewForm, field: string, key: string, level: number): HomebrewForm {
  return withField(form, field, entriesOf(form, field).map((e) => (e.child === key ? { ...e, level } : e)));
}

/** The form without the child `key` and its entries (a saved child is deleted on save; undo brings it back). */
export function withoutChild(form: HomebrewForm, key: string, fields: readonly string[]): HomebrewForm {
  let next: HomebrewForm = { ...form, children: form.children.filter((c) => c.key !== key) };
  for (const f of fields) next = withField(next, f, entriesOf(next, f).filter((e) => e.child !== key));
  return next;
}

/** The form without the entry `index` of field `field` (an element given from elsewhere). */
export const withoutEntry = (form: HomebrewForm, field: string, index: number): HomebrewForm =>
  withField(form, field, entriesOf(form, field).filter((_, i) => i !== index));

/** Problems of the child at `index`, made relative to it (`children.2.fields.name` -> `fields.name`). */
export function childProblems(problems: readonly HomebrewProblem[], index: number): HomebrewProblem[] {
  const prefix = `children.${index}.`;
  return problems.filter((p) => p.path?.startsWith(prefix)).map((p) => ({ ...p, path: p.path!.slice(prefix.length) }));
}

/** The class table's columns. */
export function columnsOf(form: HomebrewForm): HomebrewTableColumn[] {
  const v = form.fields.table;
  return Array.isArray(v) ? (v as HomebrewTableColumn[]) : [];
}

/** Columns the grid shows as counters: the 2024 proficiency bonus column is the PROF. BONUS column already. */
export const counterColumns = (columns: readonly HomebrewTableColumn[]): { column: HomebrewTableColumn; index: number }[] =>
  columns.map((column, index) => ({ column, index })).filter(({ column }) => column.key !== "proficiency_bonus");

/** A cell as typed: a whole number when it is one, else the text (dice like 1d6); empty = 0. */
export function cellValue(text: string): number | string {
  const t = text.trim();
  if (t === "") return 0;
  return /^-?\d+$/.test(t) ? Number(t) : t;
}

/** A column with 20 values. */
export const fullColumn = (c: HomebrewTableColumn): HomebrewTableColumn =>
  ({ ...c, values: LEVELS.map((_, i) => c.values[i] ?? 0) });

/** The generated features of a class (spec 4.5): the child holding each part, by the value that marks it. */
export interface Generated {
  asi?: HomebrewForm;
  subclass?: HomebrewForm;
  epicBoon?: HomebrewForm;
}

export function generatedOf(form: HomebrewForm): Generated {
  const out: Generated = {};
  for (const c of form.children) {
    const f = c.fields;
    if (!out.asi && Array.isArray(f.asiLevels) && f.asiLevels.length > 0) out.asi = c;
    else if (!out.subclass && typeof f.subclassLabel === "string" && f.subclassLabel.trim()) out.subclass = c;
    else if (!out.epicBoon && f.epicBoon === true) out.epicBoon = c;
  }
  return out;
}

/** A new child key no form of the draft has (the backend's are `n1`, `n2`...; the UI's `u1`...). */
export function freshKey(form: HomebrewForm): string {
  const used = new Set<string>();
  const walk = (f: HomebrewForm) => { if (f.key) used.add(f.key); f.children.forEach(walk); };
  walk(form);
  for (let n = 1; ; n += 1) if (!used.has(`u${n}`)) return `u${n}`;
}
