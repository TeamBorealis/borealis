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

  it("Appearance marks the theme the phone is showing: Light while System follows a light phone", async () => {
    const user = userEvent.setup();
    start("system", true);
    await user.click((await screen.findAllByRole("button", { name: "Settings" }))[0]!);
    await user.click(await screen.findByRole("tab", { name: /Appearance/i }));
    const light = await screen.findByRole("radio", { name: "Light" });
    expect(light.getAttribute("aria-checked")).toBe("true");
    expect(screen.getByRole("radio", { name: "Dark" }).getAttribute("aria-checked")).toBe("false");
  });
});
