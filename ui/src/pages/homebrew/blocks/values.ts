import type { HomebrewBlock, HomebrewBlockField, HomebrewBlockSpec } from "../../../api/contract";
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

/** The value a new block starts with for a field: required closed choices take their first choice, everything else is unset. */
export function defaultValue(f: HomebrewBlockField): unknown {
  if (f.control === "variant" && f.variants?.length) return f.required ? defaultVariant(f.variants[0]!.value, f) : undefined;
  if (!f.required) return undefined;
  switch (f.control) {
    case "number": return f.min ?? 1;
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

/** Rows whose `auto` id is numbered 1..n (a written list's items, spec 6.1). */
export function numberRows(rowsField: HomebrewBlockField, rows: readonly Fields[]): Fields[] {
  const auto = (rowsField.fields ?? []).find((f) => f.control === "auto");
  return auto ? rows.map((r, i) => ({ ...r, [auto.key]: String(i + 1) })) : [...rows];
}

/** A rule object (`{kind, ...}`) as a block, and back (a toggle's effects are nested rules, spec 6.2). */
export function ruleToBlock(rule: Fields, key: string): HomebrewBlock {
  const { kind, ...fields } = rule;
  return { key, kind: String(kind ?? ""), sentence: "", fields };
}

export const blockToRule = (b: HomebrewBlock): Fields => ({ kind: b.kind, ...b.fields });
