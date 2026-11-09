import { afterEach, describe, expect, it } from "vitest";
import { KB_OPEN_PX, installKeyboardInset } from "../lib/keyboardInset";
import { parseRules, readCss } from "./cssRules";

// Tester B15 (B-131): with the number keyboard open, the status line, Next selection and the tab bar stayed above it and
// covered about 120 px of the list. On a phone they hide while the keyboard is open and come back when it closes.
function fakeWindow(innerHeight: number) {
  const vv = Object.assign(new EventTarget(), { height: innerHeight, offsetTop: 0 });
  const win = Object.assign(new EventTarget(), { innerHeight, visualViewport: vv });
  const open = (kb: number) => { vv.height = innerHeight - kb; vv.dispatchEvent(new Event("resize")); };
  return { win, open };
}

const shellRules = () => parseRules(readCss("styles/phone/shell.css"));
const ruleFor = (sel: string) => shellRules().find((r) => r.selectors.includes(sel) && r.atRules.length === 0);

describe("bottom chrome hides while the soft keyboard is open (B15)", () => {
  const root = document.documentElement;
  afterEach(() => { root.style.removeProperty("--kb"); root.classList.remove("kb-open"); });

  it("marks the root kb-open while the keyboard is up and clears it on close", () => {
    const { win, open } = fakeWindow(852);
    const off = installKeyboardInset(win as unknown as Window, root);
    expect(root.classList.contains("kb-open")).toBe(false);
    open(336);
    expect(root.classList.contains("kb-open")).toBe(true);
    open(0);
    expect(root.classList.contains("kb-open")).toBe(false);
    open(336);
    off();
    expect(root.classList.contains("kb-open")).toBe(false);
  });

  it("a small inset (a browser toolbar, not a keyboard) does not hide the chrome", () => {
    const { win, open } = fakeWindow(852);
    const off = installKeyboardInset(win as unknown as Window, root);
    open(KB_OPEN_PX - 1);
    expect(root.classList.contains("kb-open")).toBe(false);
    open(KB_OPEN_PX);
    expect(root.classList.contains("kb-open")).toBe(true);
    off();
  });

  it("the phone layer hides the status line and the tab bar under kb-open, and lifts the shell by the whole keyboard", () => {
    expect(ruleFor("html.phone.kb-open .aur-window.narrow .aur-status")?.body).toMatch(/display: none/);
    expect(ruleFor("html.phone.kb-open .aur-bottombar")?.body).toMatch(/display: none/);
    expect(ruleFor("html.phone.kb-open .aur-window")?.body).toMatch(/padding-bottom: var\(--kb\)/);
  });
});
