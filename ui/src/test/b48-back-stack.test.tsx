import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Dialog } from "../components/controls/Dialog";
import { Flyout } from "../components/controls/Flyout";
import { closeTopOverlay, openOverlayCount } from "../components/controls/overlayStack";
import { installAndroidBack, type BackRegister } from "../lib/androidBack";

/** A Tauri `onBackButtonPress` stand-in: records the handler while registered. */
function fakeRegister() {
  const state: { handler: ((p: { canGoBack: boolean }) => void) | null; registrations: number; unregisters: number } = { handler: null, registrations: 0, unregisters: 0 };
  const register: BackRegister = async (handler) => {
    state.handler = handler;
    state.registrations++;
    return { unregister: async () => { state.handler = null; state.unregisters++; } };
  };
  return { state, register, press: () => act(() => { state.handler?.({ canGoBack: true }); }) };
}

const flush = () => act(async () => { await Promise.resolve(); await Promise.resolve(); });

function Stack() {
  const [sheet, setSheet] = useState(true);
  const [confirm, setConfirm] = useState(true);
  return (
    <>
      {sheet && <Flyout title="Menu" onClose={() => setSheet(false)}><p>menu body</p></Flyout>}
      {confirm && <Dialog title="Delete character" onClose={() => setConfirm(false)}><p>sure?</p></Dialog>}
    </>
  );
}

describe("overlay back stack (B-48 WP1, spec sec. 5.10)", () => {
  afterEach(() => { cleanup(); });

  it("every open Dialog and Flyout is on the stack, newest on top; closing the top closes only that one", () => {
    render(<Stack />);
    expect(openOverlayCount()).toBe(2);
    act(() => { closeTopOverlay(); });
    expect(screen.queryByRole("dialog", { name: "Delete character" })).toBeNull();
    expect(screen.getByRole("dialog", { name: "Menu" })).toBeTruthy();
    expect(openOverlayCount()).toBe(1);
    act(() => { closeTopOverlay(); });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(openOverlayCount()).toBe(0);
    expect(closeTopOverlay()).toBe(false);
  });

  it("Escape still closes the top overlay and the stack follows", () => {
    render(<Stack />);
    fireEvent.keyDown(screen.getByRole("dialog", { name: "Delete character" }), { key: "Escape" });
    expect(openOverlayCount()).toBe(1);
  });

  it("Android back closes the top sheet and does not leave the page", async () => {
    const back = fakeRegister();
    const off = installAndroidBack(back.register);
    window.location.hash = "#/characters";
    const historyBack = vi.spyOn(window.history, "back");
    render(<Stack />);
    await flush();
    expect(back.state.registrations).toBe(1);
    back.press();
    expect(screen.queryByRole("dialog", { name: "Delete character" })).toBeNull();
    expect(screen.getByRole("dialog", { name: "Menu" })).toBeTruthy();
    back.press();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(historyBack).not.toHaveBeenCalled();
    expect(window.location.hash).toBe("#/characters");
    await flush();
    // Nothing open: the listener is gone, so Android's own back (go back, then to the background) applies again.
    expect(back.state.handler).toBeNull();
    expect(back.state.unregisters).toBe(1);
    off();
    historyBack.mockRestore();
  });

  it("registers the back listener only while something is open", async () => {
    const back = fakeRegister();
    const off = installAndroidBack(back.register);
    await flush();
    expect(back.state.registrations).toBe(0);
    const { unmount } = render(<Flyout title="Menu" onClose={() => undefined}><p>x</p></Flyout>);
    await flush();
    expect(back.state.registrations).toBe(1);
    unmount();
    await flush();
    expect(back.state.handler).toBeNull();
    off();
  });

  it("uses the latest onClose of an overlay (state captured after it opened)", () => {
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = render(<Dialog title="Edit" onClose={first}><p>x</p></Dialog>);
    rerender(<Dialog title="Edit" onClose={second}><p>x</p></Dialog>);
    act(() => { closeTopOverlay(); });
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });
});
