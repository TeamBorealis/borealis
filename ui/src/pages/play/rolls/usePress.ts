import { type KeyboardEvent, type MouseEvent, type PointerEvent, useEffect, useRef } from "react";

/** How long a touch must rest on a roll button before the roll menu opens. */
export const LONG_PRESS_MS = 450;
/** A finger that moves farther than this is scrolling, not pressing. */
const MOVE_TOLERANCE_PX = 10;
/** The click a touch device sends after a long press is not a tap. */
const SUPPRESS_CLICK_MS = 800;

export interface PressHandlers {
  onPointerDown: (e: PointerEvent<HTMLElement>) => void;
  onPointerMove: (e: PointerEvent<HTMLElement>) => void;
  onPointerUp: () => void;
  onPointerCancel: () => void;
  onPointerLeave: () => void;
  onClick: (e: MouseEvent<HTMLElement>) => void;
  onContextMenu: (e: MouseEvent<HTMLElement>) => void;
  onKeyDown: (e: KeyboardEvent<HTMLElement>) => void;
}

/**
 * One button, two actions (GitHub #14): a tap or click (or Enter / Space) does the main action; a long press on touch,
 * a right-click, the context-menu key or Shift+F10 opens the menu (Advantage / Normal / Disadvantage) at the button.
 */
export function usePress(onTap: (el: HTMLElement) => void, onMenu: (anchor: HTMLElement) => void): PressHandlers {
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const start = useRef<{ x: number; y: number } | null>(null);
  const suppressUntil = useRef(0);
  const clear = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  };
  useEffect(() => clear, []);
  const openMenu = (el: HTMLElement) => {
    clear();
    suppressUntil.current = Date.now() + SUPPRESS_CLICK_MS;
    onMenu(el);
  };
  return {
    onPointerDown: (e) => {
      if (e.pointerType !== "touch" && e.pointerType !== "pen") return;
      const el = e.currentTarget;
      start.current = { x: e.clientX, y: e.clientY };
      clear();
      timer.current = setTimeout(() => openMenu(el), LONG_PRESS_MS);
    },
    onPointerMove: (e) => {
      const s = start.current;
      if (!s || !timer.current) return;
      if (Math.hypot(e.clientX - s.x, e.clientY - s.y) > MOVE_TOLERANCE_PX) clear();
    },
    onPointerUp: clear,
    onPointerCancel: clear,
    onPointerLeave: clear,
    onClick: (e) => {
      if (Date.now() < suppressUntil.current) {
        e.preventDefault();
        return;
      }
      onTap(e.currentTarget);
    },
    onContextMenu: (e) => {
      e.preventDefault();
      // Android sends contextmenu for the same long press the timer already answered
      if (Date.now() < suppressUntil.current) return;
      openMenu(e.currentTarget);
    },
    onKeyDown: (e) => {
      if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
        e.preventDefault();
        openMenu(e.currentTarget);
      }
    },
  };
}
