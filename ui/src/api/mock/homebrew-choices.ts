import type { HomebrewBlock, HomebrewBlockField, HomebrewBlockSpec, HomebrewProblem } from "../types";
import { SKILL_NAME_PICKER, skillKey } from "./homebrew-skills";

// The web mock's stand-in for the backend's closed-choice check (`borealis_authoring::check::fields`: `value_findings`,
// `Finding::BlockChoice`, code `block.choice`), item I0Yxk. The block codec writes combo and chips values as they are,
// so a value its field can't hold reaches the check, which says "<block>: pick <field> from the list.":
// - a combo value that is not one of the choices (any value of another shape too), unless the field is open;
// - a chips value that is not a list of choices (`blocks::fit::value_fits_with`);
// - a skill field (its picker is "Skill Name", HC16) also takes the name of a loaded homebrew skill, any case and spacing
//   (`blocks::fit::is_skill`, `skills`: the loaded skills' statistic keys);
// - a variant of a kind the field doesn't have;
// - the parts of groups, rows, variants and map values, by the part's label; a toggle's effects, by their block's label.
// The readers that show fields verbatim (`rule_kinds` "notes_d (fields shown verbatim)") take a single choice where the
// block writes a list (`blocks::fit::misfits_with`), at a block's own fields only.

/** The rule kinds whose reader shows every field verbatim (`borealis_content::rule_kinds`, `NOTES_GENERIC`). */
const VERBATIM_KINDS: readonly string[] = [
  "at_will_spells", "cantrip_swap", "free_cast_choice", "pact_weapon", "proficiency_filter", "replace_on_level", "spell_cast_alt_cost",
  "spell_replace_on_level",
];

type Obj = Record<string, unknown>;

/** A value outside its field's choices: where, and the field's label. */
interface Misfit { path: string; field: string }

const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);
const at = (path: string, key: string | number): string => `${path}.${key}`;

/** One text value of a combo or chips field: a choice, any text in an open field, or a loaded skill's name. */
function isChoice(f: HomebrewBlockField, v: unknown, skills: readonly string[]): boolean {
  if (typeof v !== "string") return false;
  if (f.allowCustom || (f.choices ?? []).some((c) => c.value === v)) return true;
  if (!f.picker?.types.includes(SKILL_NAME_PICKER)) return false;
  const key = skillKey(v);
  return key !== null && skills.includes(key);
}

/** The values of `fields` (an object's parts) outside their choices, by each part's field. */
function partMisfits(parts: readonly HomebrewBlockField[], fields: Obj, path: string, cx: Cx): Misfit[] {
  return Object.entries(fields).flatMap(([key, v]) => {
    const f = parts.find((p) => p.key === key);
    return f ? valueMisfits(f, v, at(path, key), f.label, cx) : [];
  });
}

/** The catalog, the loaded skills' keys and the element the problems are of. */
interface Cx { catalog: readonly HomebrewBlockSpec[]; skills: readonly string[]; elementId: string | null }

/** Where one value at `path` (of the field labelled `label`) is outside its choices. */
function valueMisfits(f: HomebrewBlockField, v: unknown, path: string, label: string, cx: Cx): Misfit[] {
  if (v === undefined || v === null) return [];
  const bad: Misfit[] = [{ path, field: label }];
  switch (f.control) {
    case "combo": return isChoice(f, v, cx.skills) ? [] : bad;
    case "chips": return Array.isArray(v) && v.every((x) => isChoice(f, x, cx.skills)) ? [] : bad;
    case "group": return isObj(v) ? partMisfits(f.fields ?? [], v, path, cx) : [];
    case "rows": return Array.isArray(v) ? v.flatMap((row, i) => (isObj(row) ? partMisfits(f.fields ?? [], row, at(path, i), cx) : [])) : [];
    case "variant": {
      const variant = isObj(v) ? f.variants?.find((x) => x.value === v.type) : undefined;
      if (!isObj(v) || !variant) return bad;
      const { type: _type, ...parts } = v;
      return partMisfits(variant.fields, parts, path, cx);
    }
    case "map": {
      const valueField = f.fields?.[1];
      return isObj(v) && valueField ? Object.entries(v).flatMap(([k, x]) => valueMisfits(valueField, x, at(path, k), label, cx)) : [];
    }
    default: return [];
  }
}

/** A toggle's effects: each nested block's own problems, at `<path>.<i>.fields`. */
function effectProblems(v: unknown, path: string, cx: Cx): HomebrewProblem[] {
  if (!Array.isArray(v)) return [];
  return v.flatMap((b, i) => {
    const spec = isObj(b) && isObj(b.fields) ? cx.catalog.find((s) => s.kind === b.kind) : undefined;
    return spec && isObj(b) && isObj(b.fields) ? fieldsChoiceProblems(spec, b.fields, at(at(path, i), "fields"), cx) : [];
  });
}

/** A block's label as the start of a sentence ("Gives ..." -> "Gives"), as `check::fields` writes it. */
const blockLabel = (spec: HomebrewBlockSpec): string => spec.label.replace(/[.\s]+$/, "");

/** The `block.choice` problems of one block's fields at `path` (`blocks.N.fields`). */
function fieldsChoiceProblems(spec: HomebrewBlockSpec, fields: Obj, path: string, cx: Cx): HomebrewProblem[] {
  const verbatim = VERBATIM_KINDS.includes(spec.kind);
  return spec.fields.flatMap((f) => {
    const v = fields[f.key];
    if (f.control === "blocks") return effectProblems(v, at(path, f.key), cx);
    // a verbatim reader's single value reads as the list of it
    const value = verbatim && f.control === "chips" && typeof v === "string" ? [v] : v;
    return valueMisfits(f, value, at(path, f.key), f.label, cx).map((m): HomebrewProblem => ({
      severity: "error", message: `${blockLabel(spec)}: pick ${m.field.toLowerCase()} from the list.`, path: m.path, elementId: cx.elementId, code: "block.choice",
    }));
  });
}

/** Block `i`'s values outside their closed choices, as the backend reports them (`blocks.N.fields.<path>`). `skills`:
 * the statistic keys of the loaded homebrew skills (`homebrew-skills.ts` `skillKey`). */
export function blockChoiceProblems(b: HomebrewBlock, i: number, catalog: readonly HomebrewBlockSpec[], elementId: string | null,
  skills: readonly string[] = []): HomebrewProblem[] {
  const spec = catalog.find((s) => s.kind === b.kind);
  if (!spec) return [];
  return fieldsChoiceProblems(spec, b.fields, `blocks.${i}.fields`, { catalog, skills, elementId });
}
