import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Dialog, Flyout } from "../components/controls";
import { installAndroidBack, type BackRegister } from "../lib/androidBack";
import { startPhone } from "../lib/phoneStartup";

// HW1 A2 on the emulator: the Android Back button went back in the page history (or left the app)
// while a sheet stayed open, and the full-width phone compendium had no other way to close.
// ui-mobile-adaptation: "the OS back gesture closes the topmost sheet first, then pops sub-routes".
// The app's startup (startPhone) is the only place that installs the Back handling, so these tests go through it.
type BackHandler = (p: { canGoBack: boolean }) => void;
const plugin = vi.hoisted(() => ({
  handlers: [] as BackHandler[],
  unregistered: 0,
}));
vi.mock("@tauri-apps/api/app", () => ({
  onBackButtonPress: async (h: BackHandler) => {
    plugin.handlers.push(h);
    return { unregister: async () => { plugin.unregistered += 1; plugin.handlers = plugin.handlers.filter((x) => x !== h); } };
  },
}));

const ANDROID_UA = "Mozilla/5.0 (Linux; Android 15; Pixel 8 Build/AP3A; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/131.0.0.0 Mobile Safari/537.36";
const press = async (canGoBack = true) => {
  await act(async () => { for (const h of [...plugin.handlers]) h({ canGoBack }); });
};
const settle = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });
const asAndroidApp = () => vi.spyOn(window.navigator, "userAgent", "get").mockReturnValue(ANDROID_UA);

describe("Android Back closes the topmost sheet first (HW1 A2)", () => {
  let stop: () => void = () => undefined;
  beforeEach(() => {
    plugin.handlers = [];
    plugin.unregistered = 0;
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  });
  afterEach(() => {
    stop();
    stop = () => undefined;
    vi.restoreAllMocks();
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    document.documentElement.classList.remove("android", "phone");
  });

  it("an open flyout takes the Back button and closes; with none open the default Back comes back", async () => {
    asAndroidApp();
    stop = startPhone();
    const onClose = vi.fn();
    const back = vi.spyOn(window.history, "back").mockImplementation(() => undefined);
    const { unmount } = render(<Flyout title="Compendium" position="bottom" onClose={onClose}><p>body</p></Flyout>);
    await settle();
    expect(plugin.handlers).toHaveLength(1);
    await press();
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(back).not.toHaveBeenCalled();
    unmount();
    await settle();
    expect(plugin.unregistered).toBe(1);
    expect(plugin.handlers).toHaveLength(0);
  });

  it("with a dialog over a flyout, Back closes only the dialog", async () => {
    asAndroidApp();
    stop = startPhone();
    const flyoutClose = vi.fn();
    const dialogClose = vi.fn();
    render(
      <>
        <Flyout title="Menu" onClose={flyoutClose}><p>menu</p></Flyout>
        <Dialog title="Edit" onClose={dialogClose}><p>edit</p></Dialog>
      </>,
    );
    await settle();
    expect(screen.getAllByRole("dialog")).toHaveLength(2);
    await press();
    expect(dialogClose).toHaveBeenCalledTimes(1);
    expect(flyoutClose).not.toHaveBeenCalled();
  });

  it("does not take the Back button off Android or outside the app", async () => {
    // Not Android (jsdom's own user agent) inside the app.
    stop = startPhone();
    const first = render(<Flyout title="Compendium" onClose={() => undefined}><p>body</p></Flyout>);
    await settle();
    expect(plugin.handlers).toHaveLength(0);
    first.unmount();
    stop();
    // Android, but a plain browser (no Tauri).
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    asAndroidApp();
    stop = startPhone();
    render(<Flyout title="Compendium" onClose={() => undefined}><p>body</p></Flyout>);
    await settle();
    expect(plugin.handlers).toHaveLength(0);
  });

  it("a listener whose removal is still pending does not close a second sheet", async () => {
    // Tauri keeps calling a listener until its unregister resolves: with the last sheet just closed and a new one open,
    // one press reaches both listeners. Only the current one may act.
    const handlers: BackHandler[] = [];
    const register: BackRegister = async (h) => {
      handlers.push(h);
      return { unregister: () => new Promise<void>(() => undefined) };
    };
    stop = installAndroidBack(register);
    const first = render(<Flyout title="Menu" onClose={() => undefined}><p>menu</p></Flyout>);
    await settle();
    first.unmount();
    const close = vi.fn();
    render(<Dialog title="Edit" onClose={close}><p>edit</p></Dialog>);
    await settle();
    expect(handlers).toHaveLength(2);
    await act(async () => { for (const h of handlers) h({ canGoBack: true }); });
    expect(close).toHaveBeenCalledTimes(1);
  });
});
