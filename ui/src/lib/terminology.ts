import type { Ruleset } from "../api/contract";

/**
 * The ruleset's word for a character's heritage, as the host's ruleset profile names it (`builder::ruleset_profile`):
 * "Species" for the 2024 rules, "Race" for 2014. Known without a host call, so a screen can show it on first paint
 * (B-137: the Build tab flashed "Race" while the profile loaded).
 */
export const speciesTerm = (ruleset: Ruleset | null | undefined): string => (ruleset === "dnd2024" ? "Species" : "Race");

/** The engine names the species select "Race" in both editions; pickers show the ruleset's word instead (A15, B-126). */
export function slotTitle(slot: { title: string; elementType: string }, ruleset: Ruleset | null | undefined): string {
  return slot.elementType === "race" && slot.title === "Race" ? speciesTerm(ruleset) : slot.title;
}
