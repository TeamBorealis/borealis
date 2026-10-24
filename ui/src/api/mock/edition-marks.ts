import type { SlotOption } from "../contract";
import type { ElementSummary, Ruleset } from "../types";

/**
 * R6: the mock's stand-in for the engine's edition adapter. An option written only for the other edition carries
 * `edition`; the ones the real adapter would change (an older subclass on a newer class: features below level 3
 * arrive at 3, B-44) are `converted` with the adapter's notes; the rest are "used as written".
 */
const ADAPTER_NOTES: Record<string, string[]> = {
  "light-domain": ["Subclass features listed below level 3 arrive at level 3"],
};

/** Same note the engine records for other-edition content it uses unchanged. */
export const USED_AS_WRITTEN = "used as written";

export type EditionFields = Pick<SlotOption, "edition" | "converted" | "editionNotes">;

/** The edition fields of an option for a character on `ruleset` (empty when the option fits that edition). */
export function mockEditionMarks(e: Pick<ElementSummary, "id" | "rulesets">, ruleset: Ruleset): EditionFields {
  const from = e.rulesets[0];
  if (!from || e.rulesets.includes(ruleset)) return {};
  const notes = ADAPTER_NOTES[e.id];
  return notes ? { edition: from, converted: true, editionNotes: notes } : { edition: from, editionNotes: [USED_AS_WRITTEN] };
}
