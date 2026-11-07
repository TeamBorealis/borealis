import type { AbilityKey } from "../api/contract";

/** The leading dice of a damage text and the modifier after them (`1d8+3 slashing`). */
const LEADING_DICE = /^(\s*\d+d\d+)(?:\s*[+-]\s*\d+)?/;

const signed = (n: number): string => (n < 0 ? `${n}` : `+${n}`);

/**
 * The damage with `modifier` after its leading dice, replacing a modifier already there
 * (`1d6 piercing`, +3 -> `1d6+3 piercing`); a text that does not start with dice is left as typed.
 * The host's `borealis_character::with_modifier` (GitHub #29).
 */
export function withModifier(damage: string, modifier: number): string {
  if (!LEADING_DICE.test(damage)) return damage;
  return damage.replace(LEADING_DICE, (_all, dice: string) => `${dice.trim()}${modifier ? signed(modifier) : ""}`);
}

/** What a hand-made attack worked out from `ability` shows: the modifier plus proficiency, and the dice plus the modifier. */
export function calculatedAttack(damage: string, ability: AbilityKey, modifiers: Record<AbilityKey, number>, proficiency: number): { attackBonus: string; damage: string } {
  const m = modifiers[ability] ?? 0;
  return { attackBonus: signed(m + proficiency), damage: withModifier(damage, m) };
}
