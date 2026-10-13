import type { MockPlay } from "./play";
import type {
  AbilityKey, AbilityMethod, AbilityRecord, Attack, CharacterDetails, CharacterOptions, Coins,
  InventoryItem, RollRecord, Ruleset,
} from "../types";

export interface ClassSlotState {
  slotId: string;
  /** Per-level hit point inputs (index 0 = first level taken in this class). */
  hpRolls: number[];
}

export interface MockCharacter {
  id: string;
  name: string;
  ruleset: Ruleset;
  group: string;
  favorite: boolean;
  portrait: string | null;
  path: string | null;
  updatedAt: string;
  dirty: boolean;
  xp: number;
  options: CharacterOptions;
  /** Cross-edition mode: null/undefined = ruleset default (strict for 2014, open for 2024). */
  allowCrossEdition?: boolean | null;
  method: AbilityMethod;
  base: AbilityRecord<number>;
  arrayAssign: AbilityRecord<number | null>;
  rolls: RollRecord[];
  classes: ClassSlotState[];
  /** One entry per character level: the class slot that gained it. */
  levelOrder: string[];
  choices: Record<string, string>;
  /** Picks replaced per slot id, oldest first (spell retraining). */
  slotHistory?: Record<string, { id: string; level: number }[]>;
  prepared: Record<string, string[]>;
  items: InventoryItem[];
  coins: Coins;
  storageNames: [string, string];
  details: CharacterDetails;
  attacks: Attack[];
  /** 2024 play state (resources, toggles, exhaustion, ...). */
  play?: MockPlay;
  companionName: string;
  companionPortrait: string | null;
  sourceRestrictions: string[] | null;
  notices: string[];
  warnings: string[];
}

export interface RuleSettings {
  pointBuyBudget: number;
  minBase: number;
  maxBase: number;
  capEnabled: boolean;
}

export const DEFAULT_RULES: RuleSettings = { pointBuyBudget: 27, minBase: 3, maxBase: 30, capEnabled: true };

export const XP_THRESHOLDS = [0, 300, 900, 2700, 6500, 14000, 23000, 34000, 48000, 64000, 85000, 100000, 120000, 140000, 165000, 195000, 225000, 265000, 305000, 355000];

export const POINT_BUY_COSTS: Record<number, number> = {
  8: 0, 9: 1, 10: 2, 11: 3, 12: 4, 13: 5, 14: 7, 15: 9, 16: 11, 17: 13, 18: 15, 19: 17, 20: 19,
};
export const STANDARD_ARRAY = [15, 14, 13, 12, 10, 8];

export const EMPTY_DETAILS: CharacterDetails = {
  gender: "", playerName: "", alignment: "", deity: "", age: "", height: "", weight: "", eyes: "", skin: "", hair: "",
  features: "", backstory: "", personalityTraits: "", ideal: "", bond: "", flaw: "", trinket: "", notesLeft: "", notesRight: "",
  organization: "", allies: "", combatNotes: "", sheetAcNote: "", treasure: "", questItems: "",
};

export const uniformScores = (v: number): AbilityRecord<number> => ({ str: v, dex: v, con: v, int: v, wis: v, cha: v });
export const noAssign = (): AbilityRecord<number | null> => ({ str: null, dex: null, con: null, int: null, wis: null, cha: null });

export const mod = (score: number): number => Math.floor((score - 10) / 2);
export const signed = (n: number): string => (n >= 0 ? `+${n}` : String(n));

export const levelOf = (c: MockCharacter, slotId: string): number => c.levelOrder.filter((s) => s === slotId).length;
export const totalLevel = (c: MockCharacter): number => c.levelOrder.length;

export function pointBuySpent(base: AbilityRecord<number>, keys: readonly AbilityKey[]): number {
  return keys.reduce((sum, k) => sum + (POINT_BUY_COSTS[base[k]] ?? Math.max(0, 19 + (base[k] - 20) * 2)), 0);
}
