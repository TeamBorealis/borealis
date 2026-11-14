import type { HomebrewForm } from "../../../api/contract";
import { type CasterType, isSpellcastingColumn, levelsOf, slotGrid, topSlotLevel } from "../caster/slots";
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

/** One extra column of the table: its heading and its cell at each level 1-20. */
interface Column { heading: string; cells: string[] }

/**
 * Im8nQ: a 2014 caster's spellcasting columns, which live in its Spellcasting feature (the caster type or a custom table)
 * and not in the class table, as the SRD 5.1 class tables write them: Cantrips Known, Spells Known (a known caster),
 * then the slots of each slot level the class reaches, or a pact caster's Spell Slots and Slot Level. None for a class
 * without spellcasting, or whose table already holds its spellcasting columns (a 2024 class).
 */
function casterColumns(form: HomebrewForm): Column[] {
  const caster = form.children.find((c) => c.fields.casts === true);
  if (!caster || columnsOf(form).some((c) => isSpellcastingColumn(c.key))) return [];
  const counts = (key: string): number[] => levelsOf(caster.fields[key]);
  const out: Column[] = [];
  if (counts("cantrips").some((n) => n > 0)) out.push({ heading: "Cantrips Known", cells: counts("cantrips").map(cell) });
  if (caster.fields.preparation === "known") out.push({ heading: "Spells Known", cells: counts("known").map(cell) });
  const type = (typeof caster.fields.casterType === "string" ? caster.fields.casterType : "") as CasterType;
  const grid = slotGrid(type, caster.fields.slotTable);
  if (type === "pact") {
    const slot = (row: number[]) => row.findIndex((n) => n > 0);
    out.push({ heading: "Spell Slots", cells: grid.map((row) => cell(row[slot(row)])) });
    out.push({ heading: "Slot Level", cells: grid.map((row) => (slot(row) < 0 ? NONE : ordinal(slot(row) + 1))) });
    return out;
  }
  for (let k = 0; k < topSlotLevel(grid); k++) out.push({ heading: ordinal(k + 1), cells: grid.map((row) => cell(row[k])) });
  return out;
}

/**
 * The class table of `form` as rows of cell texts, the header first: Level, Proficiency Bonus, Features (the features at
 * each level in grid order; the ability score improvement at each of its levels), then the class table's columns and a
 * 2014 caster's spellcasting columns ({@link casterColumns}).
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
  const casting = casterColumns(form);
  const header = ["Level", "Proficiency Bonus", "Features", ...columns.map(({ column }) => column.label || column.key), ...casting.map((c) => c.heading)];
  const rows = LEVELS.map((level) => [
    ordinal(level), `+${proficiencyBonus(level)}`, features(level), ...columns.map(({ column }) => cell(column.values[level - 1])),
    ...casting.map((c) => c.cells[level - 1] ?? NONE),
  ]);
  return [header, ...rows];
}
