import type { HomebrewForm, HomebrewProblem } from "../types";

// The background form's parts the web mock does the way `borealis-authoring` does (WB-14, spec 4.3): the sentences a
// 2024 background says at its fields while it lacks its abilities, origin feat or skills (`codec::background`), and
// the feat's own choices the origin feat picker offers (`FEAT_PARAMETER_PICKER`). The mock's sample feats have no
// rules, so the SRD Magic Initiate's two choices stand in.

/** The pseudo picker type of a feat's own choices (`borealis_authoring::form::FEAT_PARAMETER_PICKER`). */
export const FEAT_PARAMETER_PICKER = "Feat Parameter";

/** The SRD feats' own choices (`select_param` names and options), by the mock's feat id. */
const FEAT_PARAMETERS: Record<string, [string, string[]][]> = {
  "magic-initiate": [["Spell List", ["Cleric", "Druid", "Wizard"]], ["Spellcasting Ability", ["Intelligence", "Wisdom", "Charisma"]]],
};

const paramKey = (name: string): string => name.trim().toLowerCase().replace(/[ -]/g, "_");

/** `["Feat Parameter", <feat id>]`: the feat's choice names; with a third word, that choice's options. */
export function featParameterWords(types: readonly string[]): string[] {
  const choices = FEAT_PARAMETERS[types[1] ?? ""] ?? [];
  const param = types[2];
  if (param === undefined) return choices.map(([n]) => n);
  return choices.find(([n]) => paramKey(n) === paramKey(param))?.[1] ?? [];
}

const count = (v: unknown): number => (Array.isArray(v) ? v.length : 0);

/** A new or edited 2024 background's form problems (`codec::background`, code `form`). */
export function backgroundProblems(form: HomebrewForm, elementId: string | null): HomebrewProblem[] {
  if (form.kind !== "background" || form.fields.edition !== "dnd2024") return [];
  const at = (key: string, message: string): HomebrewProblem => ({ severity: "error", path: `fields.${key}`, elementId, code: "form", message });
  const out: HomebrewProblem[] = [];
  if (count(form.fields.abilityScores) !== 3) out.push(at("abilityScores", "Pick three abilities."));
  const feat = (form.fields.originFeat as { feat?: { id?: string } | null } | undefined)?.feat;
  if (!feat?.id) out.push(at("originFeat", "Choose the origin feat."));
  if (count(form.fields.skills) !== 2) out.push(at("skills", "Pick two skills."));
  if (count(form.fields.tools) > 1) out.push(at("tools", "A 2024 background gives one tool proficiency. Remove the others."));
  return out;
}
