import type { AbilityKey } from "../types";
import { ABILITY_NAMES } from "./content";

/**
 * Tester B5 (B-130) in the mock host. By default a mock 2024 background raises its abilities through two picks (+2, +1).
 * The scenario switch `?mockAbilityOptions=1` (or the `backgroundAbilityList` option) offers them as the engine does
 * instead: one list of options named "Intelligence +2, Charisma +1" (web e2e of the option's description).
 */
let abilityList = false;

export function setBackgroundAbilityList(on: boolean): void { abilityList = on; }
export const backgroundAbilityList = (): boolean => abilityList;

const BY_NAME = new Map((Object.entries(ABILITY_NAMES) as [AbilityKey, string][]).map(([k, name]) => [name, k]));

/** The increases a chosen option element's name carries ("Intelligence +2, Charisma +1"). */
export function optionIncreases(name: string): [AbilityKey, number][] {
  return name.split(",").flatMap((part) => {
    const m = /^\s*(\w+) \+(\d)\s*$/.exec(part);
    const key = m ? BY_NAME.get(m[1]!) : undefined;
    return m && key ? [[key, Number(m[2])] as [AbilityKey, number]] : [];
  });
}
