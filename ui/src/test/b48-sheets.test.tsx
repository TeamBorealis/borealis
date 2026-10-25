import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { renderApp } from "./utils";
import { ToastProvider, useToast } from "../components/Toast";

// B-48 WP3 sheets and toasts on phones (U9a picks O4, O5). Desktop (phone off) keeps today's behaviour.
const fast = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });

/** jsdom has no matchMedia: answer the narrow-shell query as a phone screen would. */
function narrowScreen(narrow = true, prefersLight = false) {
  const original = window.matchMedia;
  window.matchMedia = ((q: string) => ({
    matches: q.includes("max-width: 899px") ? narrow : q.includes("prefers-color-scheme: light") ? prefersLight : false,
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

let restore: () => void = () => undefined;
afterEach(() => { restore(); setPhoneForTests(false); vi.useRealTimers(); document.documentElement.removeAttribute("data-theme"); });

describe("sheets (O4)", () => {
  beforeEach(() => { restore = narrowScreen(); });

  it("swiping the grabber down far enough cancels; a short drag springs back", async () => {
    setPhoneForTests(true);
    const user = userEvent.setup();
    const api = fast();
    renderApp("/characters", { api });
    const tile = await screen.findByRole("button", { name: /^Seraphine Vale, Level 2 / });
    await user.click(tile);
    const del = await screen.findByRole("button", { name: "Delete Seraphine Vale" });
    await user.click(del);
    const sheet = await screen.findByRole("dialog", { name: "Delete character" });
    const grabber = sheet.querySelector<HTMLElement>(".sheet-grabber")!;
    expect(grabber).toBeTruthy();
    fireEvent.pointerDown(grabber, { clientY: 500, pointerId: 1 });
    fireEvent.pointerMove(grabber, { clientY: 530, pointerId: 1 });
    expect(sheet.style.transform).toBe("translateY(30px)");
    fireEvent.pointerUp(grabber, { clientY: 530, pointerId: 1 });
    expect(screen.getByRole("dialog", { name: "Delete character" })).toBeTruthy();
    expect(sheet.style.transform).toBe("");
    fireEvent.pointerDown(grabber, { clientY: 500, pointerId: 1 });
    fireEvent.pointerMove(grabber, { clientY: 640, pointerId: 1 });
    fireEvent.pointerUp(grabber, { clientY: 640, pointerId: 1 });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Delete character" })).toBeNull());
    // Swipe down is Cancel: the character is still there.
    expect((await api.listCharacters()).some((c) => c.name.startsWith("Seraphine"))).toBe(true);
    // Focus went back to the control that opened the sheet.
    expect(document.activeElement).toBe(del);
  });

  it("focus moves into the sheet when it opens", async () => {
    setPhoneForTests(true);
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click((await screen.findAllByRole("button", { name: /new character/i }))[0]!);
    const sheet = await screen.findByRole("dialog");
    expect(sheet.contains(document.activeElement)).toBe(true);
  });

  it("desktop dialogs have no grabber and ignore drags", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click(await screen.findByRole("button", { name: /^Seraphine Vale, Level 2 / }));
    await user.click(await screen.findByRole("button", { name: "Delete Seraphine Vale" }));
    const sheet = await screen.findByRole("dialog", { name: "Delete character" });
    expect(sheet.querySelector(".sheet-grabber")).toBeNull();
    const head = sheet.querySelector<HTMLElement>(".dialog-head")!;
    fireEvent.pointerDown(head, { clientY: 100, pointerId: 1 });
    fireEvent.pointerMove(head, { clientY: 400, pointerId: 1 });
    fireEvent.pointerUp(head, { clientY: 400, pointerId: 1 });
    expect(screen.getByRole("dialog", { name: "Delete character" })).toBeTruthy();
    expect(sheet.style.transform).toBe("");
  });
});

describe("menu sheet", () => {
  beforeEach(() => { restore = narrowScreen(); });
  it("the phone menu sheet has a grabber; the narrow desktop menu does not", async () => {
    setPhoneForTests(true);
    const user = userEvent.setup();
    const { unmount } = renderApp("/characters");
    await user.click(await screen.findByRole("button", { name: "Menu" }));
    expect((await screen.findByRole("dialog", { name: "Menu" })).querySelector(".sheet-grabber")).toBeTruthy();
    unmount();
    setPhoneForTests(false);
    renderApp("/characters");
    await user.click(await screen.findByRole("button", { name: "Menu" }));
    expect((await screen.findByRole("dialog", { name: "Menu" })).querySelector(".sheet-grabber")).toBeNull();
  });
});

describe("toasts on phones", () => {
  function Pusher() {
    const toast = useToast();
    return <button type="button" onClick={() => toast.push("Imported Seraphine Vale.")}>push</button>;
  }
  it("a plain toast stays 4 s on a phone (4.5 s on desktop)", () => {
    vi.useFakeTimers();
    setPhoneForTests(true);
    render(<ToastProvider><Pusher /></ToastProvider>);
    act(() => { screen.getByRole("button", { name: "push" }).click(); });
    expect(screen.getByText("Imported Seraphine Vale.")).toBeTruthy();
    act(() => { vi.advanceTimersByTime(4001); });
    expect(screen.queryByText("Imported Seraphine Vale.")).toBeNull();
  });
});
