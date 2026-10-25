import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { applyInsets, installAndroidInsets } from "../lib/androidInsets";
import { startPhone } from "../lib/phoneStartup";

// HW1 A2 (emulator, landscape): the WebView's env(safe-area-inset-top) is 0 there while the status bar
// still covers the title bar, so MainActivity reports the system bars and the page uses the larger value.
const root = () => document.documentElement.style;
const vars = ["--sys-inset-t", "--sys-inset-r", "--sys-inset-b", "--sys-inset-l"];

describe("Android system-bar insets (HW1 A2)", () => {
  afterEach(() => {
    for (const v of vars) root().removeProperty(v);
    delete (window as unknown as Record<string, unknown>).BorealisInsets;
    delete (window as unknown as Record<string, unknown>).__borealisApplyInsets;
  });

  it("sets the CSS variables from the reported insets", () => {
    applyInsets({ t: 24, r: 41, b: 14, l: 0 });
    expect(vars.map((v) => root().getPropertyValue(v))).toEqual(["24px", "41px", "14px", "0px"]);
  });

  it("ignores anything that is not a small non-negative number", () => {
    applyInsets({ t: 24, r: 41, b: 14, l: 0 });
    applyInsets({ t: -3, r: "9px", b: 10_000, l: Number.NaN });
    expect(vars.map((v) => root().getPropertyValue(v))).toEqual(["24px", "41px", "14px", "0px"]);
    applyInsets("nonsense");
    applyInsets(null);
    expect(root().getPropertyValue("--sys-inset-t")).toBe("24px");
  });

  it("reads the bridge once at start and installs the hook MainActivity calls on every change", () => {
    (window as unknown as Record<string, unknown>).BorealisInsets = { get: () => "{\"t\":42,\"r\":0,\"b\":14,\"l\":0}" };
    installAndroidInsets();
    expect(root().getPropertyValue("--sys-inset-t")).toBe("42px");
    const hook = (window as unknown as { __borealisApplyInsets: (v: unknown) => void }).__borealisApplyInsets;
    hook({ t: 24, r: 0, b: 0, l: 41 });
    expect(root().getPropertyValue("--sys-inset-l")).toBe("41px");
  });

  it("a broken bridge answer leaves the variables unset", () => {
    (window as unknown as Record<string, unknown>).BorealisInsets = { get: () => "{not json" };
    installAndroidInsets();
    expect(root().getPropertyValue("--sys-inset-t")).toBe("");
  });

  it("the safe areas use the larger of env() and the system bars on Android, keeping the bottom floor", () => {
    const tokens = readFileSync(resolve(__dirname, "../styles/tokens.css"), "utf8");
    expect(tokens).toMatch(/:root\.android \{[^}]*--safe-t: max\(env\(safe-area-inset-top, 0px\), var\(--sys-inset-t, 0px\)\)/);
    expect(tokens).toMatch(/:root\.android \{[^}]*--safe-r: max\(env\(safe-area-inset-right, 0px\), var\(--sys-inset-r, 0px\)\)/);
    expect(tokens).toMatch(/:root\.android \{[^}]*--safe-l: max\(env\(safe-area-inset-left, 0px\), var\(--sys-inset-l, 0px\)\)/);
    expect(tokens).toMatch(/:root\.android \{[^}]*--safe-b: max\(env\(safe-area-inset-bottom, 0px\), 20px, var\(--sys-inset-b, 0px\)\)/);
  });

  it("the phone layer's 16 px gesture-bar floor still clears a reported navigation bar", () => {
    const phone = readFileSync(resolve(__dirname, "../styles/phone/tokens.css"), "utf8");
    expect(phone).toMatch(/html\.phone\.android \{[^}]*--safe-b: max\(env\(safe-area-inset-bottom, 0px\), 16px, var\(--sys-inset-b, 0px\)\)/);
  });

  it("phone startup installs the hook on Android only", () => {
    const ua = vi.spyOn(window.navigator, "userAgent", "get");
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    try {
      ua.mockReturnValue("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148");
      startPhone()();
      expect((window as unknown as Record<string, unknown>).__borealisApplyInsets).toBeUndefined();
      ua.mockReturnValue("Mozilla/5.0 (Linux; Android 15; Pixel 8 Build/AP3A; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/131.0.0.0 Mobile Safari/537.36");
      (window as unknown as Record<string, unknown>).BorealisInsets = { get: () => "{\"t\":28,\"r\":0,\"b\":0,\"l\":0}" };
      startPhone()();
      expect(root().getPropertyValue("--sys-inset-t")).toBe("28px");
    } finally {
      ua.mockRestore();
      delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
      document.documentElement.classList.remove("ios", "android", "phone");
    }
  });
});
