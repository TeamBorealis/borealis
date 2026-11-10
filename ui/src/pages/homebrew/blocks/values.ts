import type { HomebrewBlock, HomebrewBlockField, HomebrewBlockSpec, HomebrewRef } from "../../../api/contract";
import { asExprValue, isEmptyTree } from "../builders/words";

export type Fields = Record<string, unknown>;

let seq = 0;
/** A new block key: unique in the draft, never written to content (`HomebrewBlock.key`). */
export const newBlockKey = (): string => `n${Date.now().toString(36)}${(seq++).toString(36)}`;

/** "Nothing set": absent, an empty text or list, an empty object, a builder value that writes nothing. */
export function isEmptyValue(v: unknown): boolean {
  if (v === undefined || v === null) return true;
  if (typeof v === "string") return v.trim() === "";
  if (Array.isArray(v)) return v.length === 0;
  const expr = asExprValue(v);
  if (expr) {
    if (expr.value.shape === "kept") return false;
    const m = expr.value.model as unknown;
    if (typeof m === "object" && m !== null && "node" in m) return isEmptyTree(m as never);
    if (typeof m === "object" && m !== null && "root" in m) return isEmptyTree((m as { root: never }).root);
    return false;
  }
  if (typeof v === "object") return Object.values(v).every(isEmptyValue);
  return false;
}

/** The number a number field starts at: its minimum (`fallback` without one), or +1 for a signed range
 * (`attack_bonus.value` is -20..20: starting at -20 would be a penalty nobody chose). */
export function startNumber(f: Pick<HomebrewBlockField, "min" | "max">, fallback = 0): number {
  if (f.min === undefined) return fallback;
  if (f.min >= 0) return f.min;
  return Math.max(f.min, Math.min(1, f.max ?? 1));
}

/** The value a new block starts with for a field: required closed choices take their first choice, everything else is unset. */
export function defaultValue(f: HomebrewBlockField): unknown {
  if (f.control === "variant" && f.variants?.length) return f.required ? defaultVariant(f.variants[0]!.value, f) : undefined;
  if (!f.required) return undefined;
  switch (f.control) {
    case "number": return startNumber(f, 1);
    case "combo": return f.allowCustom ? "" : f.choices?.[0]?.value ?? "";
    case "group": return defaults(f.fields ?? []);
    case "rows": return [];
    case "statValue": return "1";
    default: return undefined;
  }
}

/** A variant object of `type` with its parts' defaults. */
export function defaultVariant(type: string, f: HomebrewBlockField): Fields {
  const v = f.variants?.find((x) => x.value === type);
  return { type, ...defaults(v?.fields ?? []) };
}

/** Default values of a list of fields, unset ones left out. */
export function defaults(fields: readonly HomebrewBlockField[]): Fields {
  const out: Fields = {};
  for (const f of fields) {
    const v = defaultValue(f);
    if (v !== undefined) out[f.key] = v;
  }
  return out;
}

/** A new block of a catalog kind. `list` is a select of type List (spec 6.1). */
export function newBlock(spec: HomebrewBlockSpec): HomebrewBlock {
  const fields = defaults(spec.fields);
  if (spec.kind === "list") fields.type = "List";
  return { key: newBlockKey(), kind: spec.kind, sentence: spec.label, fields };
}

/** The fields left unset are dropped (an empty text is "not set", never written as ""). */
export function cleanFields(fields: Fields): Fields {
  return Object.fromEntries(Object.entries(fields).filter(([, v]) => !isEmptyValue(v)));
}

/** Required fields with nothing in them (the dialog says so before OK). Auto fields are the creator's to write. */
export function missingRequired(spec: readonly HomebrewBlockField[], fields: Fields): HomebrewBlockField[] {
  return spec.filter((f) => f.required && f.control !== "auto" && isEmptyValue(fields[f.key]));
}

/** Rows whose `auto` id is numbered 1..n (a written list's items, spec 6.1). The backend writes the id as it is and
 * the engine reads a whole number (`ListItem.id`), so the numbers are numbers. */
export function numberRows(rowsField: HomebrewBlockField, rows: readonly Fields[]): Fields[] {
  const auto = (rowsField.fields ?? []).find((f) => f.control === "auto");
  return auto ? rows.map((r, i) => ({ ...r, [auto.key]: i + 1 })) : [...rows];
}

/** A `reference` value (`HomebrewRef`), or null for anything else. */
export function asRef(v: unknown): HomebrewRef | null {
  if (typeof v !== "object" || v === null || Array.isArray(v)) return null;
  const r = v as Partial<HomebrewRef>;
  return typeof r.id === "string" && r.id !== "" ? { id: r.id, name: String(r.name ?? ""), type: String(r.type ?? ""), ...(r.missing ? { missing: true } : {}) } : null;
}

/** A `references` value (`HomebrewRef[]`), the entries that are references. */
export const asRefs = (v: unknown): HomebrewRef[] => (Array.isArray(v) ? v.flatMap((x) => asRef(x) ?? []) : []);

/** Nested blocks (a toggle's `effects`, `HomebrewBlock[]`): the entries that are blocks. */
export const asBlocks = (v: unknown): HomebrewBlock[] =>
  Array.isArray(v) ? v.filter((x): x is HomebrewBlock => typeof x === "object" && x !== null && typeof (x as HomebrewBlock).key === "string" && typeof (x as HomebrewBlock).kind === "string") : [];

/**
 * The element type of a block's picked element, for its `auto` field `type` (a grant's, spec 6.1): the backend does not
 * compute it. The type follows the block's single `reference` field; clearing the pick clears it.
 */
export function withAutoType(spec: readonly HomebrewBlockField[], before: Fields, next: Fields): Fields {
  const auto = spec.find((f) => f.control === "auto" && f.key === "type");
  const refs = spec.filter((f) => f.control === "reference");
  if (!auto || refs.length !== 1) return next;
  const key = refs[0]!.key;
  if (next[key] === before[key]) return next;
  const ref = asRef(next[key]);
  if (ref) return ref.type ? { ...next, [auto.key]: ref.type } : next;
  const { [auto.key]: _old, ...rest } = next;
  return rest;
}
