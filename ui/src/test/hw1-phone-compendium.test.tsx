import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { installAndroidBack, type BackRegister } from "../lib/androidBack";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { renderApp } from "./utils";

// HW1 A2/A3: on a phone the Compendium is a full-height sheet with no close button. Android closes it with Back;
// iOS has no Back button, so it must close by swiping the sheet down (B-48 O4). VoiceOver cannot swipe the grabber
// (it is aria-hidden) and has no Escape key, so iOS and Android builds also give every flyout a Close button that only
// screen readers see.

/** jsdom has no matchMedia: answer the narrow-shell query as a phone screen would. */
function narrowScreen() {
  const original = window.matchMedia;
  window.matchMedia = ((q: string) => ({
    matches: q.includes("max-width: 899px"),
    media: q, onchange: null, addEventListener: () => undefined, removeEventListener: () => undefined,
    addListener: () => undefined, removeListener: () => undefined, dispatchEvent: () => false,
  })) as typeof window.matchMedia;
  return () => { window.matchMedia = original; };
}

// jsdom has no PointerEvent: a MouseEvent carries clientY and pointerId well enough for the sheet drag.
if (typeof window.PointerEvent === "undefined") {
  class FakePointerEvent extends MouseEvent {
    pointerId: number;
    constructor(type: string, init: PointerEventInit = {}) { super(type, init); this.pointerId = init.pointerId ?? 1; }
  }
  (window as unknown as { PointerEvent: typeof FakePointerEvent }).PointerEvent = FakePointerEvent;
}

async function openCompendium() {
  renderApp("/characters");
  await screen.findAllByRole("button", { name: /new character/i });
  fireEvent.keyDown(window, { key: "F1" });
  return screen.findByRole("dialog", { name: "Compendium" });
}

describe("closing the phone compendium (HW1 A2)", () => {
  let restore: () => void = () => undefined;
  beforeEach(() => { restore = narrowScreen(); setPhoneForTests(true); });
  afterEach(() => { restore(); setPhoneForTests(false); });

  it("iOS: swiping the sheet down by its grabber closes it", async () => {
    const sheet = await openCompendium();
    const grabber = sheet.querySelector<HTMLElement>(".sheet-grabber");
    expect(grabber).toBeTruthy();
    fireEvent.pointerDown(grabber!, { clientY: 80, pointerId: 1 });
    fireEvent.pointerMove(grabber!, { clientY: 260, pointerId: 1 });
    fireEvent.pointerUp(grabber!, { clientY: 260, pointerId: 1 });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Compendium" })).toBeNull());
  });

  it("iOS: the title row is a drag handle too", async () => {
    const sheet = await openCompendium();
    const head = sheet.querySelector<HTMLElement>(".flyout-head")!;
    fireEvent.pointerDown(head, { clientY: 100, pointerId: 1 });
    fireEvent.pointerMove(head, { clientY: 300, pointerId: 1 });
    fireEvent.pointerUp(head, { clientY: 300, pointerId: 1 });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Compendium" })).toBeNull());
  });

  it("Android: Back closes it", async () => {
    let handler: ((p: { canGoBack: boolean }) => void) | null = null;
    const register: BackRegister = async (h) => { handler = h; return { unregister: async () => { handler = null; } }; };
    const off = installAndroidBack(register);
    try {
      await openCompendium();
      await act(async () => { await Promise.resolve(); });
      expect(handler).toBeTruthy();
      act(() => { handler!({ canGoBack: true }); });
      await waitFor(() => expect(screen.queryByRole("dialog", { name: "Compendium" })).toBeNull());
    } finally {
      off();
    }
  });
});

describe("screen-reader Close on phone and tablet flyouts", () => {
  let restore: () => void = () => undefined;
  beforeEach(() => { restore = narrowScreen(); });
  afterEach(() => { restore(); setPhoneForTests(false); setPhoneOsForTests(null); });

  it("phone: the Compendium has a visually hidden Close button that closes it", async () => {
    setPhoneForTests(true);
    const sheet = await openCompendium();
    const close = within(sheet).getByRole("button", { name: "Close" });
    expect(close.className).toContain("sr-only");
    fireEvent.click(close);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Compendium" })).toBeNull());
  });

  it("phone: opening the sheet does not put focus on the hidden Close", async () => {
    setPhoneForTests(true);
    const sheet = await openCompendium();
    expect(within(sheet).getByRole("button", { name: "Close" })).not.toBe(document.activeElement);
  });

  it("iOS tablet outside the narrow shell still gets the Close button", async () => {
    setPhoneOsForTests("ios");
    const sheet = await openCompendium();
    expect(within(sheet).getByRole("button", { name: "Close" })).toBeTruthy();
  });

  it("desktop flyouts stay as they are: no Close button (U7)", async () => {
    const sheet = await openCompendium();
    expect(within(sheet).queryByRole("button", { name: "Close" })).toBeNull();
  });
});
