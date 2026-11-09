/** How long the page must have settled before a message counts as seen (another app may be about to cover it). */
export const SEEN_SETTLE_MS = 400;

/**
 * Runs `fn` once the page is on screen (B-148): after a phone save, Android opens the PDF viewer over the app, and a
 * toast shown meanwhile would time out unseen. Waits `settleMs`, then runs at once when the page is visible, or on the
 * next return to it.
 */
export function whenSeen(fn: () => void, settleMs = SEEN_SETTLE_MS): void {
  const check = () => {
    if (document.visibilityState === "visible") { fn(); return; }
    const onChange = () => {
      if (document.visibilityState !== "visible") return;
      document.removeEventListener("visibilitychange", onChange);
      fn();
    };
    document.addEventListener("visibilitychange", onChange);
  };
  setTimeout(check, settleMs);
}
