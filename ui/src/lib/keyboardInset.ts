/**
 * On-screen keyboard height as `--kb` on the root (mobile-facelift-spec sec. 5.4). WKWebView does not shrink the layout
 * viewport for the keyboard, and Android edge-to-edge no longer resizes either, so sheet footers and the status line add
 * `--kb` to stay above it. A focused text field is scrolled to the middle of its scroller when the keyboard opens.
 */
interface ViewportBox { height: number; offsetTop: number }

/** Pixels the keyboard covers at the bottom of the layout viewport (0 when it is closed). */
export function keyboardInset(innerHeight: number, vv: ViewportBox): number {
  return Math.max(0, Math.round(innerHeight - vv.height - vv.offsetTop));
}

const TEXT_ENTRY = 'input:not([type="checkbox"]):not([type="radio"]):not([type="range"]):not([type="button"]):not([type="submit"]), textarea, [contenteditable="true"]';

export function installKeyboardInset(win: Window, root: HTMLElement): () => void {
  const vv = win.visualViewport;
  if (!vv) return () => undefined;
  let last = -1;
  const update = () => {
    const kb = keyboardInset(win.innerHeight, vv);
    if (kb === last) return;
    const opened = kb > last && kb > 0;
    last = kb;
    root.style.setProperty("--kb", `${kb}px`);
    const focused = root.ownerDocument.activeElement;
    if (opened && focused instanceof HTMLElement && focused.matches(TEXT_ENTRY)) focused.scrollIntoView({ block: "center", inline: "nearest" });
  };
  update();
  vv.addEventListener("resize", update);
  vv.addEventListener("scroll", update);
  return () => {
    vv.removeEventListener("resize", update);
    vv.removeEventListener("scroll", update);
    root.style.setProperty("--kb", "0px");
  };
}
