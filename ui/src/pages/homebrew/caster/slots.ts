import type { HomebrewLevelCounts } from "../../../api/contract";

/**
 * The spellcasting section's tables (spec 4.6, WB-17), as `borealis-authoring::caster::section` writes them: spell slots
 * per class level for each caster type, and the class table columns the section owns. Pure: the SPELLCASTING tab shows a
 * type's slots before the backend writes them; the backend's rules are the truth.
 */

/** Slots of each slot level 1-9 at one class level. */
export type SlotRow = number[];
/** Slots at levels 1-20. */
export type SlotGrid = SlotRow[];

/** The caster types (`casterType` values). */
export type CasterType = "" | "full" | "halfUp" | "halfDown" | "third" | "pact" | "custom";

export const LEVEL_COUNT = 20;
export const SLOT_LEVELS = 9;

/** A full caster's slots by caster level 1-20 (the SRD table). */
const FULL: readonly SlotRow[] = [
  [2], [3], [4, 2], [4, 3], [4, 3, 2], [4, 3, 3], [4, 3, 3, 1], [4, 3, 3, 2], [4, 3, 3, 3, 1], [4, 3, 3, 3, 2],
  [4, 3, 3, 3, 2, 1], [4, 3, 3, 3, 2, 1], [4, 3, 3, 3, 2, 1, 1], [4, 3, 3, 3, 2, 1, 1], [4, 3, 3, 3, 2, 1, 1, 1],
  [4, 3, 3, 3, 2, 1, 1, 1], [4, 3, 3, 3, 2, 1, 1, 1, 1], [4, 3, 3, 3, 3, 1, 1, 1, 1], [4, 3, 3, 3, 3, 2, 1, 1, 1],
  [4, 3, 3, 3, 3, 2, 2, 1, 1],
];
/** Pact magic: how many slots, and their level, at class levels 1-20. */
const PACT_COUNT = [1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4];
const PACT_LEVEL = [1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5];

const padded = (row: readonly number[]): SlotRow => Array.from({ length: SLOT_LEVELS }, (_, i) => row[i] ?? 0);
const empty = (): SlotGrid => Array.from({ length: LEVEL_COUNT }, () => padded([]));

/** The caster level of a class level for a caster type (0 = no slots yet). */
function casterLevel(type: CasterType, level: number): number {
  switch (type) {
    case "full": return level;
    case "halfUp": return Math.ceil(level / 2);
    case "halfDown": return level < 2 ? 0 : Math.ceil(level / 2);
    case "third": return level < 3 ? 0 : Math.ceil(level / 3);
    default: return 0;
  }
}

/** A custom table from its form value (20 rows of 9 numbers); null when it isn't one. */
export function gridOf(value: unknown): SlotGrid | null {
  if (!Array.isArray(value) || value.length !== LEVEL_COUNT) return null;
  const rows = value.map((r) => (Array.isArray(r) && r.length === SLOT_LEVELS && r.every((n) => Number.isInteger(n) && n >= 0 && n <= 9) ? r as number[] : null));
  return rows.every((r) => r !== null) ? rows as SlotGrid : null;
}

/** Slots per class level for a caster type (pact magic: its pool, all at one slot level); a custom type reads `custom`. */
export function slotGrid(type: CasterType, custom?: unknown): SlotGrid {
  if (type === "custom") return gridOf(custom) ?? empty();
  if (type === "pact") return PACT_COUNT.map((n, i) => padded(Array.from({ length: PACT_LEVEL[i]! }, (_, k) => (k === PACT_LEVEL[i]! - 1 ? n : 0))));
  return Array.from({ length: LEVEL_COUNT }, (_, i) => {
    const cl = casterLevel(type, i + 1);
    return cl > 0 ? padded(FULL[cl - 1]!) : padded([]);
  });
}

/** The highest slot level any level of the table has (the table's last shown column). */
export const topSlotLevel = (grid: SlotGrid): number => grid.reduce((top, row) => Math.max(top, row.reduce((t, n, k) => (n > 0 ? k + 1 : t), 0)), 0);

/** The table with one cell changed (a new table: every edit is one undo step). */
export const withCell = (grid: SlotGrid, level: number, slot: number, n: number): SlotGrid =>
  grid.map((row, i) => (i === level - 1 ? row.map((x, k) => (k === slot - 1 ? n : x)) : row));

/** Class table columns the spellcasting section writes (a 2024 class's `data.progression`): the LEVELS grid shows only
 * the class's own counters. */
export function isSpellcastingColumn(key: string): boolean {
  if (["cantrips", "prepared", "pact_slots", "pact_slot_level"].includes(key)) return true;
  const m = /^slot_(\d)$/.exec(key);
  return m !== null && Number(m[1]) >= 1;
}

/** A count field's levels (20 numbers), from either shape (`number[]` or `HomebrewLevelCounts`). */
export function levelsOf(value: unknown): number[] {
  const list = Array.isArray(value) ? value : (value as HomebrewLevelCounts | null)?.levels;
  return Array.from({ length: LEVEL_COUNT }, (_, i) => (Array.isArray(list) && typeof list[i] === "number" ? list[i] as number : 0));
}

/** A count field's value with its levels replaced, keeping its shape and flags. */
export function withLevels(value: unknown, levels: number[]): unknown {
  if (Array.isArray(value) || value === undefined || value === null) return levels;
  return { ...(value as HomebrewLevelCounts), levels };
}
