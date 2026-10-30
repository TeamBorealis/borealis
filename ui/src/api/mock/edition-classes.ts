import type { ClassCounts, Ruleset, SlotEmptyReason } from "../types";
import { ELEMENTS } from "./content";
import { createCharacter } from "./library";
import type { SlotDef } from "./slots";
import { type MockCharacter, totalLevel } from "./state";

/**
 * U14 (GitHub #10) in the mock host. The mock's classes suit both editions, so a scenario switch (`?mockClassless=dnd2024`,
 * or the `classlessEdition` option) takes one edition's classes away, like a user whose 2024 books are all switched off.
 */
let classless: Ruleset | null = null;

export function setClasslessEdition(r: Ruleset | null): void { classless = r; }
export const isClassless = (r: Ruleset): boolean => classless === r;

const otherOf = (r: Ruleset): Ruleset => (r === "dnd2024" ? "dnd2014" : "dnd2024");
const classTotal = (): number => ELEMENTS.filter((e) => e.type === "class").length;

export function classCounts(): ClassCounts {
  const n = classTotal();
  return { dnd2014: isClassless("dnd2014") ? 0 : n, dnd2024: isClassless("dnd2024") ? 0 : n };
}

/** The backend's `emptyReason` for the mock: only the scenario's empty Class list has one. */
export function emptyReasonOf(d: SlotDef, c: MockCharacter): SlotEmptyReason | null {
  if (d.elementType !== "class" || d.candidates.length > 0 || c.choices[d.id] || !isClassless(c.ruleset)) return null;
  return { otherEdition: { edition: otherOf(c.ruleset), rejected: classTotal(), notAllowed: 0 }, restricted: 0 };
}

/** "Start over as 2014": a new character of `ruleset` with the old one's details, options, level and scores. */
export function startedOver(c: MockCharacter, ruleset: Ruleset, playerName: string): MockCharacter {
  const fresh = createCharacter({
    name: c.name, ruleset, startingLevel: Math.max(1, totalLevel(c)), gender: c.details.gender, abilityMethod: c.method,
    options: { ...c.options, custom: { ...c.options.custom } }, portrait: c.portrait, group: c.group,
  }, { playerName });
  return { ...fresh, base: { ...c.base }, sourceRestrictions: c.sourceRestrictions ? [...c.sourceRestrictions] : null, dirty: false, path: `/library/${fresh.id}.json` };
}
