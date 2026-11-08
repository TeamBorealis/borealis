/**
 * B3 (GitHub #35): the tag of a feat, spell, language or proficiency added in the builder for an extra the DM grants.
 * Aurora has no such tag (its extras are "Additional ..." items in the equipment list); it uses the square chip of the
 * edition and prerequisite marks, in a neutral colour, so an added pick reads as such wherever it is listed.
 */
export const ADDED_TEXT = "Added";

const ADDED_TITLE = "Added in the builder for an extra the DM grants";

export function AddedMark() {
  return <span className="ed-chip added-mark" title={ADDED_TITLE}>{ADDED_TEXT}</span>;
}
