import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { ThemeMode } from "../api/contract";
import { setPhoneForTests } from "../lib/phone";
import { renderApp } from "./utils";

// B-48 O9 B (U9a): the phone follows the system light or dark setting; a theme chosen in Settings overrides it. The desktop
// keeps its dark default under "System" (U7).

/** A matchMedia whose light/dark answer can change while the app runs, as the phone's setting does. */
function systemScheme(light: boolean) {
  const original = window.matchMedia;
  let prefersLight = light;
  const listeners = new Set<() => void>();
  window.matchMedia = ((q: string) => ({
    get matches() { return q.includes("max-width: 899px") ? true : q.includes("prefers-color-scheme: light") ? prefersLight : false; },
    media: q, onchange: null,
    addEventListener: (_: string, l: () => void) => { if (q.includes("prefers-color-scheme")) listeners.add(l); },
    removeEventListener: (_: string, l: () => void) => { listeners.delete(l); },
    addListener: () => undefined, removeListener: () => undefined, dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia;
  return {
    set(next: boolean) { prefersLight = next; act(() => listeners.forEach((l) => l())); },
    restore() { window.matchMedia = original; },
  };
}

let scheme: ReturnType<typeof systemScheme> | null = null;
afterEach(() => { scheme?.restore(); scheme = null; setPhoneForTests(false); document.documentElement.removeAttribute("data-theme"); });
const themeAttr = () => document.documentElement.getAttribute("data-theme");

function start(theme: ThemeMode, light: boolean, phone = true) {
  scheme = systemScheme(light);
  setPhoneForTests(phone);
  const api = new MockApi({ aiChunkDelayMs: 2 });
  api.presetSettings({ theme });
  return renderApp("/characters", { api });
}

describe("theme follows the system (O9)", () => {
  it("System on a phone: light when the phone is light, the dark default when it is dark", async () => {
    const { unmount } = start("system", true);
    await waitFor(() => expect(themeAttr()).toBe("light"));
    unmount();
    start("system", false);
    await waitFor(() => expect(themeAttr()).toBeNull());
  });

  it("System on a phone switches live when the phone's setting changes", async () => {
    start("system", false);
    await screen.findAllByRole("button", { name: "Settings" });
    expect(themeAttr()).toBeNull();
    scheme!.set(true);
    await waitFor(() => expect(themeAttr()).toBe("light"));
    scheme!.set(false);
    await waitFor(() => expect(themeAttr()).toBeNull());
  });

  it("a theme chosen in Settings overrides the phone's setting", async () => {
    const { unmount } = start("dark", true);
    await waitFor(() => expect(themeAttr()).toBe("dark"));
    scheme!.set(true);
    expect(themeAttr()).toBe("dark");
    unmount();
    scheme!.restore();
    start("light", false);
    await waitFor(() => expect(themeAttr()).toBe("light"));
  });

  it("the desktop keeps the dark default under System, whatever the system says", async () => {
    start("system", true, false);
    await screen.findAllByRole("button", { name: "Settings" });
    expect(themeAttr()).toBeNull();
  });

  async function openAppearance() {
    const user = userEvent.setup();
    await user.click((await screen.findAllByRole("button", { name: "Settings" }))[0]!);
    await user.click(await screen.findByRole("tab", { name: /Appearance/i }));
    await screen.findByRole("radio", { name: "Light" });
    return user;
  }
  const checked = (name: string) => screen.getByRole("radio", { name }).getAttribute("aria-checked");

  it("a phone offers System beside Light and Dark; System is the marked swatch while it follows the phone", async () => {
    start("system", true);
    await openAppearance();
    expect(checked("System")).toBe("true");
    expect(checked("Light")).toBe("false");
    expect(checked("Dark")).toBe("false");
  });

  it("picking System after Light goes back to following the phone (design call 3, 2026-10-26)", async () => {
    start("light", false);
    const user = await openAppearance();
    expect(checked("Light")).toBe("true");
    await user.click(screen.getByRole("radio", { name: "System" }));
    expect(checked("System")).toBe("true");
    await waitFor(() => expect(themeAttr()).toBeNull());
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("radio", { name: "System" })).toBeNull());
    scheme!.set(true);
    await waitFor(() => expect(themeAttr()).toBe("light"));
  });

  it("the desktop keeps Light and Dark only, with Dark marked under System (U7)", async () => {
    start("system", true, false);
    await openAppearance();
    expect(screen.queryByRole("radio", { name: "System" })).toBeNull();
    expect(checked("Dark")).toBe("true");
  });
});

describe("Android bar icons follow the effective theme (sec. 5.9)", () => {
  type BarsWindow = Window & { BorealisSystemBars?: { setLight: (light: boolean) => void; isSystemNight?: () => boolean } };
  let calls: boolean[] = [];
  const install = () => { calls = []; (window as BarsWindow).BorealisSystemBars = { setLight: (l) => { calls.push(l); } }; };
  afterEach(() => { delete (window as BarsWindow).BorealisSystemBars; });
  const last = () => calls[calls.length - 1];

  it("System: dark icons on a light phone, light icons on a dark one, switching live", async () => {
    install();
    start("system", true);
    await waitFor(() => expect(last()).toBe(true));
    scheme!.set(false);
    await waitFor(() => expect(last()).toBe(false));
  });

  it("a Settings choice that differs from the phone's wins for the bars too", async () => {
    install();
    const { unmount } = start("light", false);
    await waitFor(() => expect(themeAttr()).toBe("light"));
    expect(last()).toBe(true);
    unmount();
    scheme!.restore();
    install();
    start("dark", true);
    await waitFor(() => expect(themeAttr()).toBe("dark"));
    expect(last()).toBe(false);
  });

  it("System follows the phone's night mode as Android reports it, also when the WebView's media query does not", async () => {
    // HW (WP8): the Android System WebView kept answering prefers-color-scheme: light with the phone in dark mode.
    let night = true;
    install();
    (window as BarsWindow).BorealisSystemBars!.isSystemNight = () => night;
    start("system", true);
    await screen.findAllByRole("button", { name: "Settings" });
    await waitFor(() => expect(themeAttr()).toBeNull());
    expect(last()).toBe(false);
    night = false;
    act(() => { window.dispatchEvent(new CustomEvent("borealis-system-theme")); });
    await waitFor(() => expect(themeAttr()).toBe("light"));
    expect(last()).toBe(true);
  });

  it("the desktop never calls the bridge", async () => {
    install();
    start("light", false, false);
    await waitFor(() => expect(themeAttr()).toBe("light"));
    expect(calls).toEqual([]);
  });
});
