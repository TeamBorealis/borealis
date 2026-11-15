import type { HomebrewForm, HomebrewProblem, HomebrewRef } from "../types";

// The subclass form's parts the web mock writes the way `borealis-authoring::form::subclass` does (WB-18): picking
// the class gives the subclass the tag the class's subclass choice offers ("<Class> Subclass", 2024), the level it is
// chosen at and the levels the class gives subclass features at. The mock has no class tables: the SRD's 2024 levels.

/** The 2024 SRD classes' subclass feature levels (the subclass choice at 3). */
const SUBCLASS_LEVELS: Record<string, readonly number[]> = {
  barbarian: [3, 6, 10, 14], bard: [3, 6, 14], cleric: [3, 6, 17], druid: [3, 6, 10, 14], fighter: [3, 7, 10, 15, 18],
  monk: [3, 6, 11, 17], paladin: [3, 7, 15, 20], ranger: [3, 7, 11, 15], rogue: [3, 9, 13, 17], sorcerer: [3, 6, 14, 18],
  warlock: [3, 6, 10, 14], wizard: [3, 6, 10, 14],
};

const picked = (form: HomebrewForm): HomebrewRef | null => {
  const v = form.fields.parentClass as HomebrewRef | null | undefined;
  return v && typeof v.id === "string" && v.id.trim() ? v : null;
};

/** `refresh_subclass`: a subclass form with what it takes from the class picked now; other forms as they are. */
export function refreshSubclass(form: HomebrewForm): HomebrewForm {
  if (form.kind !== "subclass") return form;
  const parent = picked(form);
  if (!parent) return form;
  const name = parent.name.trim() || parent.id;
  const levels = SUBCLASS_LEVELS[name.toLowerCase()] ?? [3];
  return { ...form, fields: { ...form.fields, subclassTag: `${name} Subclass`, levelGained: levels[0], subclassLevels: [...levels] } };
}

/** A subclass without a class: the backend's sentence at the class field (`codec::subclass`). */
export function subclassProblems(form: HomebrewForm, elementId: string | null): HomebrewProblem[] {
  if (form.kind !== "subclass" || picked(form)) return [];
  return [{ severity: "error", path: "fields.parentClass", elementId, code: "form", message: "Choose the class this subclass belongs to: its subclass choice offers it." }];
}
