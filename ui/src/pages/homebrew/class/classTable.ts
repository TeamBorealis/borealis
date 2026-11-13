import type { HomebrewForm } from "../../../api/contract";
import { LEVELS, childOf, columnsOf, counterColumns, entriesOf, entryName, proficiencyBonus } from "./children";

// "Insert class table" (spec 4.5, 7.1): the class table a class description shows, written from the LEVELS grid.

/** A cell with nothing in it, as the SRD class tables write it. */
const NONE = "—";

/** "1st", "2nd", "3rd", "4th" ... "11th", "12th", "13th" ... "21st". */
export function ordinal(n: number): string {
  const teen = n % 100 >= 11 && n % 100 <= 13;
  const suffix = teen ? "th" : ["th", "st", "nd", "rd"][n % 10] ?? "th";
  return `${n}${suffix}`;
}

const cell = (v: number | string | null | undefined): string => (v === null || v === undefined || v === 0 || String(v).trim() === "" ? NONE : String(v));

/**
 * The class table of `form` as rows of cell texts, the header first: Level, Proficiency Bonus, Features (the features at
 * each level in grid order; the ability score improvement at each of its levels), then the class table's columns.
 */
export function classTableRows(form: HomebrewForm): string[][] {
  const entries = entriesOf(form, "levels");
  const columns = counterColumns(columnsOf(form));
  const levelsOf = (i: number): number[] => {
    const e = entries[i]!;
    const asi = childOf(form, e)?.fields.asiLevels;
    return Array.isArray(asi) && asi.length > 0 ? asi.filter((n): n is number => typeof n === "number") : [e.level ?? 1];
  };
  const features = (level: number): string => {
    const names = entries.map((e, i) => ({ e, i })).filter(({ i }) => levelsOf(i).includes(level)).map(({ e }) => entryName(form, e));
    return names.length > 0 ? names.join(", ") : NONE;
  };
  const header = ["Level", "Proficiency Bonus", "Features", ...columns.map(({ column }) => column.label || column.key)];
  const rows = LEVELS.map((level) => [
    ordinal(level), `+${proficiencyBonus(level)}`, features(level), ...columns.map(({ column }) => cell(column.values[level - 1])),
  ]);
  return [header, ...rows];
}
