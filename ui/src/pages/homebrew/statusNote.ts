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
