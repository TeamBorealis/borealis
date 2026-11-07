import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { MENUS, visibleMenus } from "../components/shell/sections";
import { resetAppUpdate, WEB_CHECK_DELAY_MS } from "../state/appUpdate";
import { openSettingsTab, renderApp } from "./utils";

// Store builds (user, 2026-11-01): the iOS app and the Google Play build never point at downloads outside the store.
// The backend says which build this is (`settings.storeBuild`); desktop, web and the sideloaded APK are unchanged.
const NEWER = { state: "done", current: "1.0.0", latest: "1.0.1", newer: true, url: "https://borealisbuilder.com/#download", releaseUrl: null, notes: null, checkedAt: "2026-10-15T09:00:00Z", updateMethod: "download" } as const;
const UNSUPPORTED = { state: "unsupported", current: "1.0.0", latest: null, newer: false, url: null, releaseUrl: null, notes: null, checkedAt: null, updateMethod: "download" } as const;
const store = (mobile = true) => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5, mobile, storeBuild: true });
const help = (menus: ReturnType<typeof visibleMenus>) => menus.find((m) => m.id === "help")!.entries.map((e) => e.label);

beforeEach(() => { resetAppUpdate(); });
afterEach(() => { vi.useRealTimers(); setPhoneForTests(false); });

/** jsdom has no matchMedia: answer the narrow-shell query as a phone screen would (as b48-shell.test.tsx does). */
function narrowScreen(): () => void {
  const original = window.matchMedia;
  window.matchMedia = ((q: string) => ({
    matches: true, media: q, onchange: null, addEventListener: () => undefined, removeEventListener: () => undefined,
    addListener: () => undefined, removeListener: () => undefined, dispatchEvent: () => false,
  })) as typeof window.matchMedia;
  return () => { window.matchMedia = original; };
}

describe("store builds: Help menu", () => {
  it("hides Releases and keeps News and Support Page", () => {
    expect(help(visibleMenus(false, true))).not.toContain("Releases");
    expect(help(visibleMenus(false, true))).toEqual(expect.arrayContaining(["News", "Support Page"]));
  });

  it("direct builds keep every Help entry", () => {
    const all = MENUS.find((m) => m.id === "help")!.entries.filter((e) => !e.ai).map((e) => e.label);
    expect(help(visibleMenus(false, false))).toEqual(all);
    expect(help(visibleMenus(false))).toContain("Releases");
  });

  it("the phone menu of a store build has no Releases entry", async () => {
    const restore = narrowScreen();
    setPhoneForTests(true);
    try {
      const user = userEvent.setup();
      renderApp("/characters", { api: store() });
      await user.click(await screen.findByRole("button", { name: "Menu" }));
      const menu = await screen.findByRole("dialog", { name: "Menu" });
      expect(within(menu).getByRole("button", { name: "News" })).toBeTruthy();
      expect(within(menu).getByRole("button", { name: "Support Page" })).toBeTruthy();
      expect(within(menu).queryByRole("button", { name: "Releases" })).toBeNull();
    } finally { restore(); }
  });

  it("the menu bar of a store build has no Releases entry", async () => {
    const user = userEvent.setup();
    renderApp("/characters", { api: store(false) });
    await user.click(await screen.findByRole("menuitem", { name: "Help" }));
    await screen.findByRole("menuitem", { name: "News" });
    expect(screen.getByRole("menuitem", { name: "Support Page" })).toBeTruthy();
    expect(screen.queryByRole("menuitem", { name: "Releases" })).toBeNull();
  });
});

describe("store builds: update notice", () => {
  it("never shows the update button, even when the backend reports a newer version", async () => {
    const api = store();
    api.appUpdate.status = { ...NEWER };
    const statusCalls = vi.spyOn(api, "appUpdateStatus");
    renderApp("/characters", { api });
    await screen.findByRole("contentinfo", { name: "Status" });
    await act(async () => { await new Promise((r) => setTimeout(r, 50)); });
    expect(screen.queryByRole("button", { name: "Update Available" })).toBeNull();
    expect(statusCalls).not.toHaveBeenCalled();
  });

  it("ignores an update event from the backend", async () => {
    const api = store(false);
    renderApp("/characters", { api });
    await screen.findByRole("contentinfo", { name: "Status" });
    api.appUpdate.status = { ...NEWER };
    await act(async () => { await api.checkAppUpdate(); });
    expect(screen.queryByRole("button", { name: "Update Available" })).toBeNull();
  });

  it("phones of a store build never ask GitHub", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const api = store();
    api.appUpdate.status = { ...UNSUPPORTED };
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("offline"));
    try {
      renderApp("/characters", { api });
      await screen.findByRole("contentinfo", { name: "Status" });
      await act(async () => { await vi.advanceTimersByTimeAsync(WEB_CHECK_DELAY_MS * 2); });
      expect(fetchSpy.mock.calls.filter(([u]) => String(u).includes("api.github.com"))).toEqual([]);
    } finally { fetchSpy.mockRestore(); }
  });

  it("a sideloaded phone build still asks GitHub after the startup delay", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const api = new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5, mobile: true });
    api.appUpdate.status = { ...UNSUPPORTED };
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("offline"));
    try {
      renderApp("/characters", { api });
      await screen.findByRole("contentinfo", { name: "Status" });
      await act(async () => { await vi.advanceTimersByTimeAsync(WEB_CHECK_DELAY_MS * 2); });
      expect(fetchSpy.mock.calls.filter(([u]) => String(u).includes("api.github.com")).length).toBe(1);
    } finally { fetchSpy.mockRestore(); }
  });
});

describe("store builds: Application Settings", () => {
  it("has no update setting or Check now", async () => {
    const user = userEvent.setup();
    renderApp("/characters", { api: store(false) });
    await openSettingsTab(user, "Saving");
    expect(await screen.findByRole("checkbox", { name: "Autosave" })).toBeTruthy();
    expect(screen.queryByRole("tab", { name: "Updates" })).toBeNull();
    expect(screen.queryByRole("checkbox", { name: "Check for new versions of Borealis" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Check now" })).toBeNull();
  });
});
