import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { setStatus } from "../state/status";
import { setPhoneForTests } from "../lib/phone";
import { renderApp, seededId } from "./utils";

// B-48 WP2/WP3 behaviour on phones (U9a picks O1, O4, O7, O8, O15). Desktop (phone off) keeps today's behaviour.
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

describe("bottom tab bar (O1)", () => {
  beforeEach(() => { restore = narrowScreen(); });

  it("on a phone every tab is tappable with no character open and leads to No character open with a button to Start", async () => {
    setPhoneForTests(true);
    const user = userEvent.setup();
    renderApp("/characters");
    const build = await screen.findByTestId("nav-build");
    expect(build.getAttribute("aria-disabled")).toBeNull();
    for (const id of ["magic", "equipment", "play"]) expect(screen.getByTestId(`nav-${id}`).tagName).toBe("A");
    await user.click(build);
    expect(await screen.findByText("No character open")).toBeTruthy();
    await user.click(screen.getByRole("link", { name: "Go to Start" }));
    expect(await screen.findAllByRole("button", { name: /new character/i })).toBeTruthy();
  });

  it("a narrow desktop window keeps today's disabled tabs (no phone behaviour)", async () => {
    renderApp("/characters");
    await screen.findByTestId("nav-start");
    expect(screen.queryByTestId("nav-build")).toBeNull();
    expect(document.querySelectorAll(".bb-item.disabled").length).toBe(4);
    expect(document.querySelector(".bb-pill")).toBeNull();
  });

  it("the current tab carries a shape cue element on phones (the Android pill wraps the icon)", async () => {
    setPhoneForTests(true);
    renderApp("/characters");
    const start = await screen.findByTestId("nav-start");
    expect(start.querySelector(".bb-pill > svg")).toBeTruthy();
    expect(start.getAttribute("aria-current")).toBe("page");
  });
});

describe("status line (O7)", () => {
  beforeEach(() => { restore = narrowScreen(); });

  it("on a phone a plain message shows for 4 s, then the line folds away but stays in the page", async () => {
    setPhoneForTests(true);
    renderApp("/characters");
    const status = await screen.findByRole("contentinfo", { name: "Status" });
    vi.useFakeTimers();
    act(() => { setStatus("Saved Seraphine Vale."); });
    expect(status.getAttribute("data-collapsed")).toBe("false");
    act(() => { vi.advanceTimersByTime(3900); });
    expect(status.getAttribute("data-collapsed")).toBe("false");
    act(() => { vi.advanceTimersByTime(200); });
    expect(status.getAttribute("data-collapsed")).toBe("true");
    expect(within(status).getByRole("status").textContent).toBe("Saved Seraphine Vale.");
    act(() => { setStatus("Your character is now level 3."); });
    expect(status.getAttribute("data-collapsed")).toBe("false");
  });

  it("never folds while a choice is pending: the gold prompt and Next selection stay", async () => {
    setPhoneForTests(true);
    const api = fast();
    const user = userEvent.setup();
    renderApp("/characters", { api });
    await user.click((await screen.findAllByRole("button", { name: /new character/i }))[0]!);
    await user.click(await screen.findByTestId("create-character"));
    const next = await screen.findByRole("button", { name: /Next selection/ });
    const status = next.closest("footer")!;
    expect(status.getAttribute("data-pending")).toBe("true");
    vi.useFakeTimers();
    act(() => { vi.advanceTimersByTime(10_000); });
    expect(status.getAttribute("data-collapsed")).toBe("false");
  });

  it("desktop keeps the status line always visible", async () => {
    renderApp("/characters");
    const status = await screen.findByRole("contentinfo", { name: "Status" });
    vi.useFakeTimers();
    act(() => { vi.advanceTimersByTime(10_000); });
    expect(status.getAttribute("data-collapsed")).toBeNull();
  });
});

describe("hamburger menu (O8, O15)", () => {
  beforeEach(() => { restore = narrowScreen(); });

  it("on a phone the menu is a sheet without Exit Application; everything else stays in order", async () => {
    setPhoneForTests(true);
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click(await screen.findByRole("button", { name: "Menu" }));
    const menu = await screen.findByRole("dialog", { name: "Menu" });
    expect(within(menu).queryByRole("button", { name: "Exit Application" })).toBeNull();
    const labels = within(menu).getAllByRole("button").map((b) => b.textContent);
    expect(labels.indexOf("Open bundle...")).toBeGreaterThan(labels.indexOf("New Character"));
    expect(menu.closest(".flyout-overlay")?.getAttribute("data-sheet")).toBe("auto");
  });

  it("a narrow desktop window keeps Exit Application", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click(await screen.findByRole("button", { name: "Menu" }));
    const menu = await screen.findByRole("dialog", { name: "Menu" });
    expect(within(menu).getByRole("button", { name: "Exit Application" })).toBeTruthy();
  });
});

describe("theme follows the phone (O9)", () => {
  it("System on a phone in light mode uses the light theme; desktop stays dark", async () => {
    restore = narrowScreen(true, true);
    setPhoneForTests(true);
    const { unmount } = renderApp("/characters");
    await waitFor(() => expect(document.documentElement.getAttribute("data-theme")).toBe("light"));
    unmount();
    setPhoneForTests(false);
    renderApp("/characters");
    await waitFor(() => expect(document.documentElement.getAttribute("data-theme")).toBeNull());
  });

  it("keeps working with a character open (smoke)", async () => {
    restore = narrowScreen();
    setPhoneForTests(true);
    const api = fast();
    renderApp("/play", { api, openCharacterId: await seededId(api, "Seraphine") });
    expect(await screen.findByTestId("nav-play")).toBeTruthy();
  });
});

