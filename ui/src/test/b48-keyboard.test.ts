import { afterEach, describe, expect, it, vi } from "vitest";
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
    const field = document.createElement("input");
    document.body.appendChild(field);
    const spy = vi.fn();
    field.scrollIntoView = spy;
    const off = installKeyboardInset(win as unknown as Window, root);
    field.focus();
    open(336);
    expect(spy).toHaveBeenCalledWith({ block: "center", inline: "nearest" });
    spy.mockClear();
    const button = document.createElement("button");
    document.body.appendChild(button);
    button.scrollIntoView = spy;
    button.focus();
    open(320);
    expect(spy).not.toHaveBeenCalled(); // only text entry brings up a keyboard
    off();
  });

  it("does nothing without a visualViewport (old WebViews)", () => {
    const root = document.documentElement;
    const off = installKeyboardInset({ innerHeight: 800 } as unknown as Window, root);
    expect(root.style.getPropertyValue("--kb")).toBe("");
    off();
  });
});
