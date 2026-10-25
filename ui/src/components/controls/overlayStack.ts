import { useEffect, useRef } from "react";

/**
 * Open overlays (Dialog, Flyout, Settings windows), newest last. The Android back button closes the top one
 * (mobile-facelift-spec sec. 5.10) through the same onClose as Cancel and Escape, so any unsaved-changes prompt still runs.
 */
interface Entry { id: number; close: () => void }

let stack: readonly Entry[] = [];
let seq = 0;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((l) => l());

/** Puts an overlay on the stack; the returned function takes it off. */
export function pushOverlay(close: () => void): () => void {
  const entry = { id: ++seq, close };
  stack = [...stack, entry];
  notify();
  return () => {
    const next = stack.filter((e) => e.id !== entry.id);
    if (next.length === stack.length) return;
    stack = next;
    notify();
  };
}

/** Closes the newest open overlay. False when nothing is open. */
export function closeTopOverlay(): boolean {
  const top = stack[stack.length - 1];
  if (!top) return false;
  top.close();
  return true;
}

export const openOverlayCount = (): number => stack.length;

export function subscribeOverlays(listener: () => void): () => void {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

/** Keeps the calling overlay on the stack while it is mounted, closing through the latest `onClose`. */
export function useOverlayEntry(onClose: () => void): void {
  const latest = useRef(onClose);
  latest.current = onClose;
  useEffect(() => pushOverlay(() => latest.current()), []);
}
