/**
 * On-screen keyboard height as `--kb` on the root (mobile-facelift-spec sec. 5.4). WKWebView does not shrink the layout
 * viewport for the keyboard, and Android edge-to-edge no longer resizes either, so sheet footers and the status line add
 * `--kb` to stay above it. A focused text field is scrolled to the middle of its scroller when the keyboard opens.
 * The Android System WebView drawn edge to edge does not shrink its visual viewport either (WP8, emulator), so MainActivity
 * also reports the IME inset (`setNativeKeyboardInset`, through androidInsets.ts); `--kb` is the larger of the two.
 */
interface ViewportBox { height: number; offsetTop: number }

/** Pixels the keyboard covers at the bottom of the layout viewport (0 when it is closed). */
export function keyboardInset(innerHeight: number, vv: ViewportBox): number {
  return Math.max(0, Math.round(innerHeight - vv.height - vv.offsetTop));
}

/** The keyboard height the platform reported (CSS px), and the installed updater that folds it into `--kb`. */
let nativeKb = 0;
let refresh: (() => void) | null = null;

/** Android: the IME inset from MainActivity (0 when the keyboard is closed). */
export function setNativeKeyboardInset(px: number): void {
  nativeKb = Math.max(0, Math.round(px));
  refresh?.();
}

const TEXT_ENTRY = 'input:not([type="checkbox"]):not([type="radio"]):not([type="range"]):not([type="button"]):not([type="submit"]), textarea, [contenteditable="true"]';

/** The nearest ancestor that scrolls vertically (the sheet body or the content area), or null. */
function scrollerOf(el: HTMLElement): HTMLElement | null {
  const view = el.ownerDocument.defaultView;
  if (!view) return null;
  for (let a = el.parentElement; a && a !== el.ownerDocument.body; a = a.parentElement) {
    const y = view.getComputedStyle(a).overflowY;
    if ((y === "auto" || y === "scroll") && a.scrollHeight > a.clientHeight) return a;
  }
  return null;
}

/**
 * Centres the focused field in its own scroller. Not `scrollIntoView`: on iOS that also scrolled the page itself (the
 * shell never scrolls), which pushed fixed sheets up so the Settings footer sat under the keyboard bar (WP8, simulator).
 * A page scroll iOS made on its own is undone; the sheets already keep above the keyboard through `--kb`.
 */
function revealInScroller(field: HTMLElement, win: Window): void {
  const scroller = scrollerOf(field);
  if (scroller) {
    const f = field.getBoundingClientRect();
    const s = scroller.getBoundingClientRect();
    scroller.scrollTop += Math.round(f.top + f.height / 2 - (s.top + s.height / 2));
  }
  if (typeof win.scrollTo === "function" && (win.scrollY ?? 0) > 0) win.scrollTo(0, 0);
}

export function installKeyboardInset(win: Window, root: HTMLElement): () => void {
  const vv = win.visualViewport;
  if (!vv) return () => undefined;
  let last = -1;
  const update = () => {
    const kb = Math.max(keyboardInset(win.innerHeight, vv), nativeKb);
    if (kb === last) return;
    const opened = kb > last && kb > 0;
    last = kb;
    root.style.setProperty("--kb", `${kb}px`);
    const focused = root.ownerDocument.activeElement;
    if (opened && focused instanceof HTMLElement && focused.matches(TEXT_ENTRY)) revealInScroller(focused, win);
  };
  update();
  refresh = update;
  vv.addEventListener("resize", update);
  vv.addEventListener("scroll", update);
  return () => {
    if (refresh === update) refresh = null;
    nativeKb = 0;
    vv.removeEventListener("resize", update);
    vv.removeEventListener("scroll", update);
    root.style.setProperty("--kb", "0px");
  };
}
