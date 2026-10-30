import type { ElementType, Ruleset, SelectionSlot } from "../../api/contract";

/** Edition year as the UI writes it. */
export const editionYear = (r: Ruleset): string => (r === "dnd2014" ? "2014" : "2024");

const NOUNS: Partial<Record<ElementType, [string, string]>> = {
  class: ["class", "classes"],
  archetype: ["subclass", "subclasses"],
  feat: ["feat", "feats"],
  race: ["option", "options"],
  spell: ["spell", "spells"],
};

const counted = (n: number, type: ElementType): string => {
  const [one, many] = NOUNS[type] ?? ["option", "options"];
  return `${n} ${n === 1 ? one : many}`;
};

interface Props {
  slot: SelectionSlot;
  /** The character's edition. */
  ruleset: Ruleset;
  onManageSources: () => void;
  /** "Start over as 2014" (only the Class list offers it). */
  onStartOver: (edition: Ruleset) => void;
  /** Opens Edition & compatibility (the character's "other edition" switch). */
  onEditionOptions: () => void;
}

/**
 * Why a pick list is empty (U14, GitHub #10), from the backend's `emptyReason`: options only another edition's character can
 * use (R4: classes never cross; the Class list offers "Start over as <year>"), options the character's "other edition" switch
 * hides, and options its source restrictions hide. "Source restrictions" is named only when they really hide something.
 */
export function EmptyListNote({ slot, ruleset, onManageSources, onStartOver, onEditionOptions }: Props) {
  const reason = slot.emptyReason;
  const other = reason?.otherEdition ?? null;
  const type = slot.elementType;
  const year = other ? editionYear(other.edition) : "";
  const offerStartOver = !!other && other.rejected > 0 && type === "class" && !slot.chosen;
  return (
    <div className="pick-empty pick-why" role="note" aria-label="Why this list is empty">
      {other ? <span>This character uses the {editionYear(ruleset)} rules.</span> : <span>Nothing matches.</span>}
      {other && other.rejected > 0 && (
        <> <span>{counted(other.rejected, type)} from {year} books {other.rejected === 1 ? "needs" : "need"} a {year} character.</span>
          {offerStartOver && <button type="button" className="pick-why-act" onClick={() => onStartOver(other.edition)}>Start over as {year}</button>}
        </>
      )}
      {other && other.notAllowed > 0 && (
        <> <span>{counted(other.notAllowed, type)} from {year} books {other.notAllowed === 1 ? "is" : "are"} hidden: this character does not allow the other edition.</span>
          <button type="button" className="pick-why-act" onClick={onEditionOptions}>Edition &amp; compatibility</button>
        </>
      )}
      {reason && reason.restricted > 0 && (
        <> <span>{counted(reason.restricted, type)} {reason.restricted === 1 ? "is" : "are"} hidden by your source restrictions.</span>
          <button type="button" className="pick-why-act" onClick={onManageSources}>Manage Sources</button>
        </>
      )}
    </div>
  );
}
