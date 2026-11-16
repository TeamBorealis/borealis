import type { HomebrewForm } from "../../api/contract";
import { setStatus } from "../../state/status";

/**
 * A message the creator writes on the status line (HC18): a save, a copy, a rule added, a list saved. The pack's workspace
 * keeps it until the form on screen changes; `count` adds the open element's problems after it once the check answers
 * ("Rule added. · 2 problems"). A message about the pack rather than the open form (a multi-select edit, SPELL LISTS)
 * takes no count.
 */
export interface StatusNote {
  readonly text: string;
  readonly count: boolean;
}

/** A creator message on the status line and the form it belongs to; `form` is undefined until it first shows. */
export interface ShownNote extends StatusNote {
  readonly form?: HomebrewForm | null;
}

/**
 * What the kept note becomes, decided by an effect that read `seen` while `form` is on screen. `current` is the note now:
 * when a newer message was written after the effect read `seen` (Save right after "Starting equipment added.": the old
 * note's clearing ran once the saved element reopened, after the save had written its message), it is kept. Otherwise the
 * note seen is bound to the form on screen when it first shows, kept while that form is on screen, and dropped once another
 * form is; with no element open it leaves the note (the workspace writes it as the plain line).
 */
export function settleNote(current: ShownNote | null, seen: ShownNote, form: HomebrewForm | null): ShownNote | null {
  if (current !== seen) return current;
  if (seen.form !== undefined) return seen.form === form ? seen : null;
  return form ? { ...seen, form } : null;
}

type Listener = (note: StatusNote) => void;
const listeners = new Set<Listener>();

/** Writes a creator message. With no workspace listening it is the plain status message. */
export function noteStatus(text: string, opts: { count?: boolean } = {}): void {
  const note: StatusNote = { text, count: opts.count ?? true };
  if (listeners.size === 0) {
    setStatus(text);
    return;
  }
  listeners.forEach((l) => l(note));
}

/** The workspace listens for the creator's messages; returns the unsubscribe. */
export function onStatusNote(listener: Listener): () => void {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}
