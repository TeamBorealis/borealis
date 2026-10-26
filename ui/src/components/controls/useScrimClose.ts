import { type MouseEvent, type PointerEvent, useRef } from "react";

export interface ScrimHandlers {
  onPointerDown: (e: PointerEvent<HTMLElement>) => void;
  onMouseDown: (e: MouseEvent<HTMLElement>) => void;
}

/**
 * A press on the scrim (the overlay itself, not the panel) closes. The press must also start there: on Android a tap
 * on a sheet's title row starts a sheet drag that captures the pointer, and Chrome then sends the tap's
 * compatibility mousedown to the scrim. A mousedown with no pointerdown before it (tests, old engines) still closes.
 */
export function useScrimClose(onClose: () => void): ScrimHandlers {
  const pressStart = useRef<EventTarget | null>(null);
  return {
    onPointerDown: (e) => { pressStart.current = e.target; },
    onMouseDown: (e) => {
      const started = pressStart.current;
      pressStart.current = null;
      if (e.target !== e.currentTarget) return;
      if (started === null || started === e.currentTarget) onClose();
    },
  };
}
