import type { HomebrewFieldSpec, HomebrewKindDescriptor, HomebrewProblem, HomebrewProblemCounts } from "../../api/contract";

/** The form field a problem points at (`fields.level` -> `level`), or null for element-wide problems and other parts. */
export function fieldKeyOf(path: string | null): string | null {
  const m = path ? /^fields\.([^.]+)/.exec(path) : null;
  return m ? m[1]! : null;
}

/** The sub-tab holding a problem's field, so PROBLEMS can jump to it; blocks live on RULES. */
export function tabOf(problem: HomebrewProblem, kind: HomebrewKindDescriptor | null): string | null {
  if (!kind) return null;
  const key = fieldKeyOf(problem.path);
  if (key) return kind.fields.find((f) => f.key === key)?.tab ?? null;
  if (problem.path?.startsWith("blocks")) return kind.tabs.find((t) => t.id === "rules")?.id ?? null;
  // a child's problem: the tab of the field that holds the children (the class LEVELS grid, WB-16)
  if (problem.path?.startsWith("children.")) return kind.fields.find((f) => f.control === "levels" || f.control === "children")?.tab ?? null;
  return null;
}

export function countProblems(problems: readonly HomebrewProblem[]): HomebrewProblemCounts {
  return {
    errors: problems.filter((p) => p.severity === "error").length,
    warnings: problems.filter((p) => p.severity === "warning").length,
  };
}

/** The status line's words (spec 8.3: "2 problems"). Info lines (balance hints) are not problems. */
export function problemsLine(c: HomebrewProblemCounts): string {
  const n = c.errors + c.warnings;
  return n === 0 ? "No problems" : `${n} ${n === 1 ? "problem" : "problems"}`;
}

/** The worst severity among a field's problems, for its label colour (8.3). */
export function fieldSeverity(problems: readonly HomebrewProblem[]): "error" | "warning" | null {
  if (problems.some((p) => p.severity === "error")) return "error";
  return problems.some((p) => p.severity === "warning") ? "warning" : null;
}

/** Problems of one field. */
export function problemsFor(field: HomebrewFieldSpec, problems: readonly HomebrewProblem[]): HomebrewProblem[] {
  return problems.filter((p) => fieldKeyOf(p.path) === field.key && p.severity !== "info");
}

/** The DOM id of a field's control (PROBLEMS and F2 focus it). */
export const fieldDomId = (key: string): string => `hb-f-${key}`;
