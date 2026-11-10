/**
 * Undo history of one draft (homebrew creator spec 10.2): a stack of immutable form states. Every edit is one step, except
 * typing: edits with the same `group` (one text field) merge into one step until the user pauses for `TYPING_PAUSE_MS` or
 * edits another field. The stack keeps the last `HISTORY_LIMIT` steps and lives in memory only.
 */
export interface History<T> {
  readonly past: readonly T[];
  readonly present: T;
  readonly future: readonly T[];
  /** The typing group the present state belongs to; null after a plain edit, an undo or a redo. */
  readonly group: string | null;
  /** When the present state was recorded (ms). */
  readonly at: number;
}

export const HISTORY_LIMIT = 200;
export const TYPING_PAUSE_MS = 700;

export interface RecordOptions {
  /** Typing in one field: edits of the same group within the pause merge into one step. */
  group?: string | null;
  now: number;
  limit?: number;
  pauseMs?: number;
}

export function startHistory<T>(present: T, now = 0): History<T> {
  return { past: [], present, future: [], group: null, at: now };
}

/** Records `next` as the new present. The same object as the present records nothing. */
export function record<T>(h: History<T>, next: T, opts: RecordOptions): History<T> {
  if (next === h.present) return h;
  const group = opts.group ?? null;
  const pause = opts.pauseMs ?? TYPING_PAUSE_MS;
  if (group !== null && h.group === group && opts.now - h.at < pause) {
    return { ...h, present: next, future: [], at: opts.now };
  }
  const limit = opts.limit ?? HISTORY_LIMIT;
  const past = [...h.past, h.present];
  return { past: past.length > limit ? past.slice(past.length - limit) : past, present: next, future: [], group, at: opts.now };
}

export function canUndo<T>(h: History<T>): boolean {
  return h.past.length > 0;
}

export function canRedo<T>(h: History<T>): boolean {
  return h.future.length > 0;
}

export function undo<T>(h: History<T>): History<T> {
  if (!canUndo(h)) return h;
  const previous = h.past[h.past.length - 1] as T;
  return { past: h.past.slice(0, -1), present: previous, future: [h.present, ...h.future], group: null, at: h.at };
}

export function redo<T>(h: History<T>): History<T> {
  if (!canRedo(h)) return h;
  const [next, ...rest] = h.future as T[];
  return { past: [...h.past, h.present], present: next as T, future: rest, group: null, at: h.at };
}
