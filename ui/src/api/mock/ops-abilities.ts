import { ABILITY_KEYS, type AbilityKey, type AbilityMethod, type RollRecord } from "../types";
import { apiError } from "./errors";
import { type MockCharacter, type RuleSettings, STANDARD_ARRAY, noAssign, pointBuySpent, uniformScores } from "./state";

export type Rng = () => number;
const d6 = (rng: Rng) => 1 + Math.floor(rng() * 6);

export function setMethod(c: MockCharacter, method: AbilityMethod): MockCharacter {
  const base = method === "point_buy" ? uniformScores(8) : method === "manual" ? c.base : uniformScores(method === "standard_array" ? 8 : 10);
  return { ...c, method, base, arrayAssign: noAssign(), rolls: [] };
}

export function setBase(c: MockCharacter, ability: AbilityKey, value: number, rules: RuleSettings): MockCharacter {
  if (c.method === "standard_array" || c.method === "roll_3d6" || c.method === "roll_4d6") {
    throw apiError("invalid_argument", "Scores are set by the chosen generation method.");
  }
  const unrestricted = c.options.pointBuyUnrestricted || c.method === "manual";
  const min = unrestricted ? rules.minBase : 8;
  const max = unrestricted ? rules.maxBase : 15;
  if (value < min || value > max) throw apiError("invalid_argument", `Base score must be between ${min} and ${max}.`);
  const base = { ...c.base, [ability]: value };
  if (c.method === "point_buy" && !c.options.pointBuyUnrestricted && pointBuySpent(base, ABILITY_KEYS) > rules.pointBuyBudget) {
    throw apiError("budget_exceeded", `Not enough points: budget is ${rules.pointBuyBudget}.`);
  }
  return { ...c, base };
}

export function rollAbilities(c: MockCharacter, rng: Rng, ability?: AbilityKey): MockCharacter {
  if (c.method !== "roll_3d6" && c.method !== "roll_4d6") throw apiError("invalid_argument", "Rolling is only available in roll modes.");
  const keys = ability ? [ability] : [...ABILITY_KEYS];
  const base = { ...c.base };
  const rolls: RollRecord[] = [...c.rolls];
  for (const k of keys) {
    const n = c.method === "roll_3d6" ? 3 : 4;
    const dice = Array.from({ length: n }, () => d6(rng));
    const kept = n === 4 ? [...dice].sort((a, b) => a - b).slice(1) : dice;
    const total = kept.reduce((a, b) => a + b, 0);
    base[k] = total;
    rolls.push({ ability: k, dice, total });
  }
  return { ...c, base, rolls };
}

/** Per-ability assignment that can neither duplicate nor lose a value (swaps on conflict). */
export function assignArray(c: MockCharacter, ability: AbilityKey, value: number | null): MockCharacter {
  if (c.method !== "standard_array") throw apiError("invalid_argument", "Standard array is not the active method.");
  if (value !== null && !STANDARD_ARRAY.includes(value)) throw apiError("invalid_argument", "Not a standard array value.");
  const assigned = { ...c.arrayAssign };
  if (value !== null) {
    const holder = ABILITY_KEYS.find((k) => assigned[k] === value && k !== ability);
    if (holder) assigned[holder] = assigned[ability];
  }
  assigned[ability] = value;
  const base = { ...c.base };
  for (const k of ABILITY_KEYS) base[k] = assigned[k] ?? 8;
  return { ...c, arrayAssign: assigned, base };
}
