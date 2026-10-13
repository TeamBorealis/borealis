import { ABILITY_KEYS, type AbilityKey, type AbilityRecord, type AbilityState } from "../types";
import { ABILITY_NAMES, getClass, getRace, SUBRACE_BONUS } from "./content";
import {
  type MockCharacter, type RuleSettings, POINT_BUY_COSTS, STANDARD_ARRAY, mod, pointBuySpent,
} from "./state";
import { classIdOf } from "./slots";

const zero = (): AbilityRecord<number> => ({ str: 0, dex: 0, con: 0, int: 0, wis: 0, cha: 0 });
const empty = (): AbilityRecord<string[]> => ({ str: [], dex: [], con: [], int: [], wis: [], cha: [] });

/** Collects bonuses (and their source labels) from race, background and ASI picks. */
export function abilityBonuses(c: MockCharacter): { bonus: AbilityRecord<number>; sources: AbilityRecord<string[]> } {
  const bonus = zero();
  const sources = empty();
  const add = (k: AbilityKey, n: number, label: string) => {
    bonus[k] += n;
    sources[k].push(`${label} ${n >= 0 ? "+" : ""}${n}`);
  };
  const race = getRace(c.choices["race:0"] ?? "");
  if (race && c.ruleset === "dnd2014") {
    for (const k of ABILITY_KEYS) if (race.bonus2014[k]) add(k, race.bonus2014[k]!, "Race");
    const sub = SUBRACE_BONUS[c.choices["subrace:0"] ?? ""];
    if (sub) for (const k of ABILITY_KEYS) if (sub[k]) add(k, sub[k]!, "Subrace");
  }
  for (const [slot, el] of Object.entries(c.choices)) {
    const m = /^asi:(\w+):(\d)$/.exec(el);
    if (!m) continue;
    const label = slot.startsWith("bg:") ? "Background" : "Improvement";
    add(m[1] as AbilityKey, Number(m[2]), label);
  }
  return { bonus, sources };
}

export function deriveAbilities(c: MockCharacter, rules: RuleSettings): AbilityState {
  const { bonus, sources } = abilityBonuses(c);
  const cap = rules.capEnabled ? 20 : 30;
  const final = zero();
  const modifier = zero();
  const overCap: AbilityKey[] = [];
  for (const k of ABILITY_KEYS) {
    const raw = c.base[k] + bonus[k];
    if (raw > cap) overCap.push(k);
    final[k] = Math.min(raw, cap);
    modifier[k] = mod(final[k]);
  }
  const mainClass = getClass(classIdOf(c, c.classes[0]?.slotId ?? ""));
  const prof = 2 + Math.floor((c.levelOrder.length - 1) / 4);
  const save = {} as AbilityState["save"];
  for (const k of ABILITY_KEYS) {
    const proficient = mainClass?.saves.includes(k) ?? false;
    save[k] = { bonus: modifier[k] + (proficient ? prof : 0), proficient };
  }
  const spent = pointBuySpent(c.base, ABILITY_KEYS);
  const unrestricted = c.options.pointBuyUnrestricted;
  const assigned = c.arrayAssign;
  const used = new Set(Object.values(assigned).filter((v): v is number => v !== null));
  return {
    method: c.method, base: c.base, bonus, bonusSources: sources, final, modifier, save, cap, overCap,
    pointBuy: c.method === "point_buy"
      ? { budget: rules.pointBuyBudget, spent, remaining: rules.pointBuyBudget - spent, costs: POINT_BUY_COSTS,
          min: unrestricted ? rules.minBase : 8, max: unrestricted ? rules.maxBase : 15, unrestricted }
      : null,
    standardArray: c.method === "standard_array"
      ? { values: STANDARD_ARRAY, assigned, unassigned: STANDARD_ARRAY.filter((v) => !used.has(v)) }
      : null,
    rolls: c.rolls,
  };
}

export const abilityLabel = (k: AbilityKey): string => ABILITY_NAMES[k];
