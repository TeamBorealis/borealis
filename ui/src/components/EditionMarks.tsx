import type { MouseEvent } from "react";
import type { SlotOption } from "../api/contract";

/**
 * R6 / R6a: picker edition marks, a deliberate exception to "match Aurora exactly".
 *
 * Rule: a row gets the edition badge ("2014" / "2024") only when the option was written for the other edition than
 * the character's (the backend sets `edition` only then), so same-edition and edition-neutral rows stay as Aurora
 * draws them and a mixed list marks exactly its other-edition rows. The "converted" tag appears only on those rows
 * the edition adapter changed; its notes show on hover (title) and on tap/click (an inline notes line).
 */
export interface EditionMarks {
  /** The year label of the edition the option was written for, or null for a same-edition row. */
  badge: "2014" | "2024" | null;
  /** The edition adapter changed the option. */
  converted: boolean;
  /** The adapter's notes (what changed, or "used as written"). */
  notes: string[];
}

type MarkSource = Pick<SlotOption, "edition" | "converted" | "editionNotes">;

const NO_MARKS: EditionMarks = { badge: null, converted: false, notes: [] };

/** The edition marks of an option row (see the rule above). */
export function editionMarks(o: Partial<MarkSource>): EditionMarks {
  if (!o.edition) return NO_MARKS;
  return { badge: o.edition === "dnd2024" ? "2024" : "2014", converted: o.converted === true, notes: o.editionNotes ?? [] };
}

/** Hover text of the edition badge. */
export function badgeTitle(m: EditionMarks): string {
  if (!m.badge) return "";
  const head = `Written for the ${m.badge} rules`;
  return !m.converted && m.notes.length > 0 ? `${head}; ${m.notes.join("; ")}` : head;
}

/** Hover text (and accessible name) of the "converted" tag: the adapter's notes. */
export function convertedTitle(m: EditionMarks): string {
  if (!m.converted || !m.badge) return "";
  const head = `Converted from the ${m.badge} rules`;
  return m.notes.length > 0 ? `${head}: ${m.notes.join("; ")}` : head;
}

/** The small edition badge after an option's source; renders nothing on same-edition rows. */
export function EditionBadge({ marks }: { marks: EditionMarks }) {
  if (!marks.badge) return null;
  return <span className="ed-badge" title={badgeTitle(marks)}>{marks.badge}</span>;
}

interface TagProps {
  marks: EditionMarks;
  /** The notes line is shown. */
  open: boolean;
  onToggle: () => void;
  /** Id of the notes element this tag controls. */
  notesId: string;
}

/** The "converted" tag: hover shows the adapter's notes, tap/click toggles them inline (no hover on touch). */
export function ConvertedTag({ marks, open, onToggle, notesId }: TagProps) {
  if (!marks.converted || !marks.badge) return null;
  const stop = (e: MouseEvent) => e.stopPropagation();
  return (
    <button type="button" className="ed-tag" title={convertedTitle(marks)} aria-label={convertedTitle(marks)}
      aria-expanded={open} aria-controls={notesId} onMouseDown={stop} onDoubleClick={stop}
      onClick={(e) => { e.stopPropagation(); onToggle(); }}>
      <span className="ed-chip" aria-hidden="true">Converted</span>
    </button>
  );
}

/** The adapter's notes under a converted row, shown while its "converted" tag is toggled open. */
export function EditionNotes({ marks, id, open, className = "" }: { marks: EditionMarks; id: string; open: boolean; className?: string }) {
  if (!marks.converted || !marks.badge) return null;
  const notes = marks.notes.length > 0 ? marks.notes : ["Changed by the edition adapter."];
  return (
    <span id={id} className={`ed-notes ${className}`.trim()} role="note" hidden={!open}>
      <span className="ed-notes-head">Converted from the {marks.badge} rules:</span> {notes.join("; ")}
    </span>
  );
}
