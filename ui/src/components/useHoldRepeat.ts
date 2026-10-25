import { type PointerEvent, useCallback, useEffect, useRef } from "react";

/** A press held this long starts repeating (B-48 WP5, mobile-facelift-spec sec. 3.4 Steppers). */
export const HOLD_DELAY_MS = 400;
/** Then one step per interval until the finger lifts. */
export const HOLD_REPEAT_MS = 100;

export interface HoldHandlers {
  onPointerDown: (e: PointerEvent<HTMLElement>) => void;
  onPointerUp: () => void;
  onPointerLeave: () => void;
  onPointerCancel: () => void;
  onClick: () => void;
}

/**
 * Press-and-hold repeat for stepper buttons on touch and pen. A tap (or a mouse click) steps once through `onClick`; a held
 * touch steps every {@link HOLD_REPEAT_MS} after {@link HOLD_DELAY_MS}, and the click that ends a repeating hold adds no
 * extra step. Mouse presses never repeat, so desktop behaviour is unchanged. `step` should read current values through a
 * ref: it is captured when the press starts.
 */
export function useHoldRepeat(): (step: () => void) => HoldHandlers {
  const delay = useRef<ReturnType<typeof setTimeout> | null>(null);
  const repeat = useRef<ReturnType<typeof setInterval> | null>(null);
  const repeated = useRef(false);
  const stop = useCallback(() => {
    if (delay.current) clearTimeout(delay.current);
    if (repeat.current) clearInterval(repeat.current);
    delay.current = null;
    repeat.current = null;
  }, []);
  useEffect(() => stop, [stop]);
  return useCallback((step: () => void): HoldHandlers => ({
    onPointerDown: (e) => {
      stop();
      repeated.current = false;
      if (e.pointerType === "mouse") return;
      delay.current = setTimeout(() => {
        delay.current = null;
        repeat.current = setInterval(() => { repeated.current = true; step(); }, HOLD_REPEAT_MS);
      }, HOLD_DELAY_MS);
    },
    onPointerUp: stop,
    onPointerLeave: stop,
    onPointerCancel: stop,
    onClick: () => {
      if (repeated.current) { repeated.current = false; return; }
      step();
    },
  }), [stop]);
}
