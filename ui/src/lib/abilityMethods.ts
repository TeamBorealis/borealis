import { ABILITY_KEYS, type AbilityMethod, type AbilityRecord } from "../api/contract";
import { methodLabel } from "./format";

/** The ability generation methods in the New Character flyout's order (also the Abilities tab's control, B-97). */
export const ABILITY_METHODS: readonly AbilityMethod[] = ["point_buy", "standard_array", "roll_4d6", "roll_3d6", "manual"];

/** The standard array, STR..CHA, in the order the engine assigns it when the method is chosen. */
export const STANDARD_ARRAY: readonly number[] = [15, 14, 13, 12, 10, 8];

/** Point buy starts every ability at 8. */
export const POINT_BUY_START = 8;

export interface AbilityMethodOption { value: AbilityMethod; label: string }

export const abilityMethodOptions: readonly AbilityMethodOption[] = ABILITY_METHODS.map((m) => ({
  value: m,
  label: m === "standard_array" ? `Standard Array (${STANDARD_ARRAY.join(", ")})` : methodLabel(m),
}));

const fromList = (values: readonly number[]): AbilityRecord<number> =>
  Object.fromEntries(ABILITY_KEYS.map((k, i) => [k, values[i]!])) as AbilityRecord<number>;

/**
 * The base scores after `set_ability_method` (decisions-engine B-11): point buy resets to all 8s, the standard array
 * to 15, 14, 13, 12, 10, 8 in order; manual and the roll methods keep the current scores.
 */
export function scoresAfterMethodChange(base: AbilityRecord<number>, method: AbilityMethod): AbilityRecord<number> {
  if (method === "point_buy") return fromList(ABILITY_KEYS.map(() => POINT_BUY_START));
  if (method === "standard_array") return fromList(STANDARD_ARRAY);
  return base;
}

/** True when switching to `method` would change at least one base score. */
export const methodChangeAltersScores = (base: AbilityRecord<number>, method: AbilityMethod): boolean => {
  const next = scoresAfterMethodChange(base, method);
  return ABILITY_KEYS.some((k) => next[k] !== base[k]);
};

/** Plain words for the confirmation: what happens to the scores. */
export function methodChangeMessage(method: AbilityMethod): string {
  if (method === "point_buy") return `Your base scores go back to ${POINT_BUY_START}, the point buy starting point. Bonuses from your species, background and other choices stay.`;
  const values = `${STANDARD_ARRAY.slice(0, -1).join(", ")} and ${STANDARD_ARRAY[STANDARD_ARRAY.length - 1]}`;
  return `Your base scores are replaced by the standard array, ${values} from Strength to Charisma. You can swap them afterwards. Bonuses from your species, background and other choices stay.`;
}
