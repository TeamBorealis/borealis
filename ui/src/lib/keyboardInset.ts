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
    if (opened && focused instanceof HTMLElement && focused.matches(TEXT_ENTRY)) focused.scrollIntoView({ block: "center", inline: "nearest" });
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
