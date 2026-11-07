/**
 * HR3 (GitHub #24): the mark of a feat or multiclass taken without meeting its prerequisites, under the Borealis house
 * rules "Ignore feat prerequisites" / "Ignore multiclass prerequisites". Like the edition marks, a deliberate exception
 * to Aurora parity (Aurora has no such rule): the small square warning chip of the "converted" tag, so the sheet stays
 * honest about an override wherever the pick is shown.
 */
export const PREREQ_UNMET_TEXT = "prerequisites not met";

const PREREQ_UNMET_TITLE = "Taken under a house rule that ignores prerequisites";

export function PrereqUnmetMark() {
  return <span className="ed-chip prereq-mark" title={PREREQ_UNMET_TITLE}>{PREREQ_UNMET_TEXT}</span>;
}
