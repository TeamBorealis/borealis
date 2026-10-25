import { type PointerEvent, type RefObject, useRef } from "react";

/** Where a drag may start: the grabber and the sheet's title row (not its buttons). */
const HANDLE = ".sheet-grabber, .dialog-head, .flyout-head, .aur-set-head";
const INTERACTIVE = "button, a, input, select, textarea, [role=button], [role=tab]";
/** A drag this far down dismisses; a flick past FLICK_PX that is fast enough dismisses too. */
const DISMISS_PX = 96;
const FLICK_PX = 48;
const FLICK_PX_PER_MS = 0.6;
const SETTLE_MS = 180;

export interface SheetDragHandlers {
  onPointerDown?: (e: PointerEvent<HTMLElement>) => void;
  onPointerMove?: (e: PointerEvent<HTMLElement>) => void;
  onPointerUp?: (e: PointerEvent<HTMLElement>) => void;
  onPointerCancel?: (e: PointerEvent<HTMLElement>) => void;
}

/**
 * B-48 O4: swipe a sheet down by its grabber or title row to cancel it (the same onClose as Cancel and Escape, so any
 * unsaved-changes prompt still runs). A short drag springs back. Phones only: `enabled` false returns no handlers.
 */
export function useSheetDrag(sheet: RefObject<HTMLElement | null>, onClose: () => void, enabled: boolean): SheetDragHandlers {
  const drag = useRef<{ y: number; t: number; dy: number } | null>(null);
  if (!enabled) return {};
  const move = (dy: number) => {
    const el = sheet.current;
    if (el) el.style.transform = dy > 0 ? `translateY(${dy}px)` : "";
  };
  const end = (e: PointerEvent<HTMLElement>) => {
    const d = drag.current;
    if (!d) return;
    drag.current = null;
    const el = sheet.current;
    el?.classList.remove("sheet-dragging");
    const dy = Math.max(0, e.clientY - d.y);
    const speed = dy / Math.max(1, e.timeStamp - d.t);
    const settle = () => {
      if (!el?.isConnected) return;
      el.classList.add("sheet-settling");
      setTimeout(() => el.classList.remove("sheet-settling"), SETTLE_MS);
      move(0);
    };
    if (dy > DISMISS_PX || (dy > FLICK_PX && speed > FLICK_PX_PER_MS)) {
      onClose();
      // A sheet that stays open (say it asked about unsaved changes first) springs back.
      setTimeout(settle, 0);
      return;
    }
    settle();
  };
  return {
    onPointerDown: (e) => {
      const target = e.target as HTMLElement;
      if (!target.closest(HANDLE) || target.closest(INTERACTIVE)) return;
      drag.current = { y: e.clientY, t: e.timeStamp, dy: 0 };
      sheet.current?.classList.add("sheet-dragging");
      try { e.currentTarget.setPointerCapture?.(e.pointerId); } catch { /* the pointer may already be gone */ }
    },
    onPointerMove: (e) => {
      const d = drag.current;
      if (!d) return;
      d.dy = Math.max(0, e.clientY - d.y);
      move(d.dy);
    },
    onPointerUp: end,
    onPointerCancel: end,
  };
}
