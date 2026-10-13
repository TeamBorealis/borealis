import { type KeyboardEvent, useEffect, useRef } from "react";

const FOCUSABLE = 'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';

/** Focus trap + Escape + focus restore shared by Dialog and Flyout. */
export function useModalBehavior(onClose: () => void) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    const node = ref.current;
    node?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    if (!node?.contains(document.activeElement)) node?.querySelector<HTMLElement>(FOCUSABLE)?.focus();
    return () => prev?.focus?.();
  }, []);
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key !== "Tab" || !ref.current) return;
    const items = [...ref.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
    if (!items.length) return;
    const first = items[0]!;
    const last = items[items.length - 1]!;
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
  };
  return { ref, onKeyDown };
}
