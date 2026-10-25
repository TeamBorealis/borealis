import { afterEach, describe, expect, it } from "vitest";
import { installKeyboardInset, keyboardInset } from "../lib/keyboardInset";

/** A window with a visualViewport the test resizes, as the on-screen keyboard does. */
function fakeWindow(innerHeight: number) {
  const vv = Object.assign(new EventTarget(), { height: innerHeight, offsetTop: 0 });
  const win = Object.assign(new EventTarget(), { innerHeight, visualViewport: vv });
  const open = (kb: number, offsetTop = 0) => { vv.height = innerHeight - kb - offsetTop; vv.offsetTop = offsetTop; vv.dispatchEvent(new Event("resize")); };
  return { win, vv, open };
}

describe("--kb keyboard inset (B-48 WP1, spec sec. 5.4)", () => {
  afterEach(() => { document.documentElement.style.removeProperty("--kb"); document.body.innerHTML = ""; });

  it("is innerHeight - visualViewport.height - offsetTop, never negative", () => {
    expect(keyboardInset(852, { height: 852, offsetTop: 0 })).toBe(0);
    expect(keyboardInset(852, { height: 516, offsetTop: 0 })).toBe(336);
    expect(keyboardInset(852, { height: 500, offsetTop: 40 })).toBe(312);
    expect(keyboardInset(852, { height: 900, offsetTop: 0 })).toBe(0);
  });

  it("sets --kb on the root while the keyboard is open and back to 0px when it closes", () => {
    const root = document.documentElement;
    const { win, open } = fakeWindow(852);
    const off = installKeyboardInset(win as unknown as Window, root);
    expect(root.style.getPropertyValue("--kb")).toBe("0px");
    open(336);
    expect(root.style.getPropertyValue("--kb")).toBe("336px");
    open(0);
    expect(root.style.getPropertyValue("--kb")).toBe("0px");
    off();
    open(300);
    expect(root.style.getPropertyValue("--kb")).toBe("0px");
  });

  it("keeps a focused field in view when the keyboard opens", () => {
    const root = document.documentElement;
    const { win, open } = fakeWindow(852);
    // WP8: centred inside its own scroller (not scrollIntoView, which also scrolled the page on iOS).
    const scroller = document.createElement("div");
    scroller.style.overflowY = "auto";
    Object.defineProperty(scroller, "scrollHeight", { value: 1000 });
    Object.defineProperty(scroller, "clientHeight", { value: 300 });
    let top = 0;
    Object.defineProperty(scroller, "scrollTop", { get: () => top, set: (v: number) => { top = v; } });
    const rect = (t: number, h: number) => () => ({ top: t, height: h, bottom: t + h, left: 0, right: 300, width: 300, x: 0, y: t, toJSON: () => ({}) });
    scroller.getBoundingClientRect = rect(0, 300);
    const field = document.createElement("input");
    field.getBoundingClientRect = rect(500, 40);
    scroller.appendChild(field);
    document.body.appendChild(scroller);
    const off = installKeyboardInset(win as unknown as Window, root);
    field.focus();
    open(336);
    expect(top).toBe(370);
    const button = document.createElement("button");
    button.getBoundingClientRect = rect(800, 40);
    scroller.appendChild(button);
    button.focus();
    open(320);
    expect(top).toBe(370); // only text entry brings up a keyboard
    off();
  });

  it("does nothing without a visualViewport (old WebViews)", () => {
    const root = document.documentElement;
    const off = installKeyboardInset({ innerHeight: 800 } as unknown as Window, root);
    expect(root.style.getPropertyValue("--kb")).toBe("");
    off();
  });
});
