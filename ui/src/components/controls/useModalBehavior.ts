import { type KeyboardEvent, useEffect, useRef } from "react";
import { useOverlayEntry } from "./overlayStack";
import { phoneOs } from "../../lib/phone";

const FOCUSABLE = 'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';
const NOT_TYPED = new Set(["checkbox", "radio", "button", "submit", "reset", "range", "color", "file", "image", "hidden"]);

/** A field that opens the on-screen keyboard when it takes focus. */
function opensKeyboard(el: HTMLElement): boolean {
  return el instanceof HTMLTextAreaElement || el.isContentEditable || (el instanceof HTMLInputElement && !NOT_TYPED.has(el.type));
}

/**
 * The element an opening dialog or sheet focuses: its `[data-autofocus]`, else the first focusable that is not a
 * screen-reader-only Close (`data-no-autofocus`). On iOS and Android a text field is never chosen: focusing it while
 * the sheet rises opened the keyboard and scrolled the sheet so the field was off screen (iPhone New Character).
 */
function openingFocus(node: HTMLElement, mobileOs: boolean): HTMLElement | undefined {
  const allowed = (el: HTMLElement) => !(mobileOs && opensKeyboard(el));
  const preferred = node.querySelector<HTMLElement>("[data-autofocus]");
  if (preferred && allowed(preferred)) return preferred;
  return [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].find((el) => !el.hasAttribute("data-no-autofocus") && allowed(el));
}

/** Focus trap + Escape + focus restore shared by Dialog and Flyout; the overlay also joins the back stack (Android back). */
export function useModalBehavior(onClose: () => void) {
  const ref = useRef<HTMLDivElement>(null);
  useOverlayEntry(onClose);
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    const node = ref.current;
    if (node) openingFocus(node, phoneOs() !== null)?.focus();
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
