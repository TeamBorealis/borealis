import type { HomebrewBlock, HomebrewBlockField, HomebrewBlockSpec, HomebrewProblem } from "../types";

// The web mock's stand-in for the backend's block codec (`borealis_authoring::form::CatalogBlockCodec`, the WB-04/WB-05
// join): every block field value must have the shape of its control (`HomebrewBlockValueByControl`), or the backend
// refuses it at `blocks.N.fields.<path>`. The mock checks the same shapes so that a control writing the wrong one fails
// on the web too. The Rust sample form (`api/__fixtures__/homebrew-form.json`) is the reference: it passes unchanged.

/** The backend's sentence for a value it can't read (`form::codec::decode`). */
export const SHAPE_MESSAGE = "This value can't be read. Undo the last change or enter it again.";

/** The gate fields every gated block has besides its catalog fields (spec 6). */
const GATE: Record<string, HomebrewBlockField> = {
  level: { key: "level", label: "From level", tab: "rules", control: "number", required: false, bindings: ["level"] },
  requirements: { key: "requirements", label: "Only if", tab: "rules", control: "condition", required: false, bindings: ["requirements"] },
  // "Also gained again at levels" (WB-16 `form::AGAIN_AT`): a class feature's block read from copies at other levels
  againAt: { key: "againAt", label: "Also gained again at levels", tab: "rules", control: "numbers", required: false, bindings: [] },
};

type Obj = Record<string, unknown>;

const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);
const isNum = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const isStr = (v: unknown): v is string => typeof v === "string";
const listOf = (v: unknown, each: (x: unknown) => boolean): boolean => Array.isArray(v) && v.every(each);
const isRef = (v: unknown): boolean => isObj(v) && isStr(v.id) && v.id !== "" && isStr(v.name) && isStr(v.type);
const isExpr = (v: unknown): boolean => isObj(v) && isObj(v.value) && (v.value.shape === "model" || v.value.shape === "kept");

/** Shapes of the controls whose value has no parts (`HomebrewBlockValueByControl`). */
const SIMPLE: Partial<Record<HomebrewBlockField["control"], (v: unknown, f: HomebrewBlockField) => boolean>> = {
  text: isStr, multiline: isStr, dice: isStr, combo: isStr, statistic: isStr, statValue: isStr, classCounter: isStr,
  number: (v, f) => isNum(v) || (isStr(v) && !!(f.choices?.length || f.allowCustom)),
  numbers: (v) => listOf(v, isNum),
  check: (v) => typeof v === "boolean",
  chips: (v) => listOf(v, isStr),
  reference: isRef,
  references: (v) => listOf(v, isRef),
  name: (v, f) => (f.multiple ? listOf(v, isStr) : isStr(v)),
  numberOrStatistic: (v) => isNum(v) || (isObj(v) && v.type === "stat" && isStr(v.stat)),
  condition: isExpr, tagFilter: isExpr, equipped: isExpr, formula: isExpr,
  formulas: (v) => listOf(v, isExpr),
  uses: (v) => isNum(v) || isStr(v) || (isObj(v) && v.type === "table" && isStr(v.key)),
  abilitySource: (v) => isStr(v) || (isObj(v) && isStr(v.param)),
  abilityChoice: (v) => isNum(v) || listOf(v, isStr),
  splits: (v) => listOf(v, (row) => listOf(row, isNum)),
};

const at = (path: string, key: string | number): string => (path ? `${path}.${key}` : String(key));

/** Problems of the parts of one object (a block's fields, a group, a row, a variant), by their fields. */
function partsProblems(fields: readonly HomebrewBlockField[], v: Obj, path: string, catalog: readonly HomebrewBlockSpec[], extra: Record<string, HomebrewBlockField> = {}): string[] {
  return Object.entries(v).flatMap(([key, x]) => {
    const f = fields.find((y) => y.key === key) ?? extra[key];
    if (!f) return [at(path, key)];
    return valueProblems(f, x, at(path, key), catalog);
  });
}

/** Paths (relative to the block's fields) where a value has the wrong shape for its control. */
function valueProblems(f: HomebrewBlockField, v: unknown, path: string, catalog: readonly HomebrewBlockSpec[]): string[] {
  if (v === undefined || v === null) return [];
  const simple = SIMPLE[f.control];
  if (simple) return simple(v, f) ? [] : [path];
  switch (f.control) {
    case "auto": return [];
    case "group": return isObj(v) ? partsProblems(f.fields ?? [], v, path, catalog) : [path];
    case "rows":
      return Array.isArray(v) ? v.flatMap((row, i) => (isObj(row) ? rowProblems(f.fields ?? [], row, at(path, i), catalog) : [at(path, i)])) : [path];
    case "variant": {
      const variant = isObj(v) ? f.variants?.find((x) => x.value === v.type) : undefined;
      if (!isObj(v) || !variant) return [path];
      const { type: _type, ...parts } = v;
      return partsProblems(variant.fields, parts, path, catalog);
    }
    case "map": {
      const valueField = f.fields?.[1];
      return isObj(v) && valueField ? Object.entries(v).flatMap(([k, x]) => valueProblems(valueField, x, at(path, k), catalog)) : [path];
    }
    case "blocks":
      return Array.isArray(v) ? v.flatMap((b, i) => nestedProblems(b, f, at(path, i), catalog)) : [path];
    default: return [];
  }
}

/** One row: its parts, and its `auto` number (a written list's row id, which the engine reads as a whole number). */
function rowProblems(fields: readonly HomebrewBlockField[], row: Obj, path: string, catalog: readonly HomebrewBlockSpec[]): string[] {
  const numbers = fields.filter((f) => f.control === "auto" && row[f.key] !== undefined && !Number.isInteger(row[f.key])).map((f) => at(path, f.key));
  return [...partsProblems(fields, row, path, catalog), ...numbers];
}

/** A nested block (a toggle's effect): a `HomebrewBlock` of an offered kind, never with kept parts. */
function nestedProblems(b: unknown, f: HomebrewBlockField, path: string, catalog: readonly HomebrewBlockSpec[]): string[] {
  if (!isObj(b) || !isStr(b.key) || !isStr(b.kind) || !isObj(b.fields) || (Array.isArray(b.preserved) && b.preserved.length > 0)) return [path];
  const spec = catalog.find((s) => s.kind === b.kind);
  if (!spec || (f.kinds && !f.kinds.includes(spec.kind))) return [path];
  return fieldsProblems(spec, b.fields, at(path, "fields"), catalog);
}

function fieldsProblems(spec: HomebrewBlockSpec, fields: Obj, path: string, catalog: readonly HomebrewBlockSpec[]): string[] {
  const gates = { ...(spec.levelGated ? { level: GATE.level!, againAt: GATE.againAt! } : {}), ...(spec.onlyIf ? { requirements: GATE.requirements! } : {}) };
  // A written list is a select of type List: its `type` is the creator's (always "List").
  return partsProblems(spec.fields, fields, path, catalog, gates);
}

/** Block `i`'s values of the wrong shape, as the backend reports them (`blocks.N.fields.<path>`). */
export function blockShapeProblems(b: HomebrewBlock, i: number, catalog: readonly HomebrewBlockSpec[], elementId: string | null): HomebrewProblem[] {
  const spec = catalog.find((s) => s.kind === b.kind);
  if (!spec) return [];
  return fieldsProblems(spec, b.fields, "", catalog).map((p) => ({
    severity: "error", message: SHAPE_MESSAGE, path: `blocks.${i}.fields.${p}`, elementId, code: "block.shape",
  }));
}
