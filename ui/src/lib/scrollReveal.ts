/**
 * Page scrolling for the builder (A11): bring a picker to the top of its scroller, or start a tab at the top. Not
 * `scrollIntoView`: on iOS that also pans the page itself (the shell never scrolls), which pushes fixed bars out of place
 * (see keyboardInset.ts); setting the scroller's own offset moves only the list.
 */

/** Space left above a revealed element (px), so its header does not touch the scroller's edge. */
export const REVEAL_GAP_PX = 8;

/** The scroller holding `start`: `start` itself or its nearest ancestor that scrolls vertically, else the page (main.aur-content). */
export function scrollerFrom(start: HTMLElement | null): HTMLElement | null {
  const view = start?.ownerDocument.defaultView;
  for (let a = start; a && a !== a.ownerDocument.body; a = a.parentElement) {
    if (a.classList.contains("aur-content")) return a;
    const y = view?.getComputedStyle(a).overflowY;
    if (y === "auto" || y === "scroll") return a;
  }
  return null;
}

/** Scrolls `el`'s scroller so `el`'s top sits REVEAL_GAP_PX below the scroller's top edge. */
export function revealAtTop(el: HTMLElement): void {
  const scroller = scrollerFrom(el.parentElement);
  if (!scroller) return;
  const offset = el.getBoundingClientRect().top - scroller.getBoundingClientRect().top - REVEAL_GAP_PX;
  scroller.scrollTop = Math.max(0, Math.round(scroller.scrollTop + offset));
}

/** Scrolls the scroller holding `el` back to its top. */
export function scrollToStart(el: HTMLElement | null): void {
  const scroller = scrollerFrom(el);
  if (scroller) scroller.scrollTop = 0;
}
