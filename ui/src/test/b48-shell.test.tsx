import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
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

/** A seeded mock character with every required selection made. */
/** A seeded character with every pick made; `settle` also declines its starting equipment ("I'll buy my own", B-129). */
async function completeSeeded(api: MockApi, settle = true): Promise<string> {
  for (const c of await api.listCharacters()) {
    const s = await api.getSnapshot({ characterId: c.id });
    if (s.progress.remaining !== 0 || s.progress.nextSlotId !== null) continue;
    if (settle) {
      for (const p of s.play.equipmentPackages) {
        if (!p.chosen && !p.declined) await api.chooseEquipmentPackage({ characterId: c.id, packageId: p.package, option: "none" });
      }
    }
    return c.id;
  }
  throw new Error("no complete seeded character");
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
    expect(within(status).queryByRole("group", { name: "Character complete" })).toBeNull();
    vi.useFakeTimers();
    act(() => { vi.advanceTimersByTime(10_000); });
    expect(status.getAttribute("data-collapsed")).toBe("false");
  });

  // B-143 (TV1 C10, option a; U23): creation gets a "complete" moment that leads to the result. It comes when the session's
  // last open pick is made, here the seeded character's untaken starting-equipment package (B-129), declined on Gear.
  /** Opens a character whose only open pick is its starting equipment, and declines it on Gear: the last pick. */
  async function makeLastPick(api: MockApi, user: ReturnType<typeof userEvent.setup>): Promise<HTMLElement> {
    renderApp("/builder", { api, openCharacterId: await completeSeeded(api, false) });
    await user.click(await screen.findByRole("button", { name: /Next selection/ }));
    const chooser = await screen.findByRole("region", { name: "Starting equipment" });
    await user.click(within(chooser).getByRole("button", { name: /^I'll buy my own/ }));
    return screen.findByRole("group", { name: "Character complete" });
  }

  it("a character opened already complete gets no complete moment (the bottom tabs reach Sheet and Play)", async () => {
    setPhoneForTests(true);
    const api = fast();
    renderApp("/builder", { api, openCharacterId: await completeSeeded(api) });
    const status = await screen.findByRole("contentinfo", { name: "Status" });
    await waitFor(() => expect(within(status).getByRole("status").textContent).toMatch(/^Loaded/));
    expect(within(status).queryByRole("group", { name: "Character complete" })).toBeNull();
    expect(within(status).queryByRole("button", { name: /Next selection/ })).toBeNull();
  });

  it("the last pick brings the moment, which stays open; View sheet opens the preview and the moment goes", async () => {
    setPhoneForTests(true);
    const api = fast();
    const user = userEvent.setup();
    const done = await makeLastPick(api, user);
    const status = done.closest("footer")!;
    expect(within(status).queryByRole("button", { name: /Next selection/ })).toBeNull();
    vi.useFakeTimers();
    act(() => { vi.advanceTimersByTime(10_000); });
    expect(status.getAttribute("data-collapsed")).toBe("false");
    vi.useRealTimers();
    await user.click(within(done).getByRole("button", { name: "View sheet" }));
    expect(await screen.findByRole("dialog", { name: "Character sheet preview" })).toBeTruthy();
    expect(screen.queryByRole("group", { name: "Character complete" })).toBeNull();
  });

  it("an untaken starting-equipment package holds the moment back: Next selection leads to Gear (B-129)", async () => {
    setPhoneForTests(true);
    const api = fast();
    const user = userEvent.setup();
    const id = await completeSeeded(api, false);
    // One open selection besides the package: making it leaves the package open, so no moment yet.
    const before = await api.getSnapshot({ characterId: id });
    const slot = before.slots.find((s) => s.tab === "background" && !s.locked && s.chosen && s.chosen.type !== "list")!;
    await api.clearSelection({ characterId: id, slotId: slot.id });
    renderApp("/builder/background", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: `Choose ${slot.chosen!.name}` }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).progress.remaining).toBe(0));
    const next = await screen.findByRole("button", { name: /Next selection/ });
    expect(screen.queryByRole("group", { name: "Character complete" })).toBeNull();
    await user.click(next);
    const chooser = await screen.findByRole("region", { name: "Starting equipment" });
    await user.click(within(chooser).getByRole("button", { name: /^I'll buy my own/ }));
    expect(await screen.findByRole("group", { name: "Character complete" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Next selection/ })).toBeNull();
  });

  it("Play in the moment opens Play; back on Build the moment does not come back", async () => {
    setPhoneForTests(true);
    const api = fast();
    const user = userEvent.setup();
    await user.click(within(await makeLastPick(api, user)).getByRole("button", { name: "Play" }));
    expect(await screen.findByRole("group", { name: "Rests" })).toBeTruthy();
    expect(screen.queryByRole("group", { name: "Character complete" })).toBeNull();
    await user.click(screen.getByTestId("nav-build"));
    await screen.findByRole("heading", { name: "Build" });
    expect(screen.queryByRole("group", { name: "Character complete" })).toBeNull();
  });

  it("dismissed, the moment goes and stays away on the other creation tabs; the line folds as usual", async () => {
    setPhoneForTests(true);
    const api = fast();
    const user = userEvent.setup();
    const done = await makeLastPick(api, user);
    const status = done.closest("footer")!;
    vi.useFakeTimers();
    fireEvent.click(within(done).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("group", { name: "Character complete" })).toBeNull();
    expect(status.getAttribute("data-collapsed")).toBe("false");
    act(() => { vi.advanceTimersByTime(4_100); });
    expect(status.getAttribute("data-collapsed")).toBe("true");
    vi.useRealTimers();
    await user.click(screen.getByTestId("nav-build"));
    await screen.findByRole("heading", { name: "Build" });
    expect(screen.queryByRole("group", { name: "Character complete" })).toBeNull();
  });

  it("leaving the creation tabs takes the moment away, and coming back does not bring it back", async () => {
    setPhoneForTests(true);
    const api = fast();
    const user = userEvent.setup();
    await makeLastPick(api, user);
    await user.click(screen.getByTestId("nav-start"));
    await waitFor(() => expect(screen.queryByRole("group", { name: "Character complete" })).toBeNull());
    await user.click(screen.getByTestId("nav-build"));
    await screen.findByRole("heading", { name: "Build" });
    expect(screen.queryByRole("group", { name: "Character complete" })).toBeNull();
  });

  it("a desktop window, at any width, never shows the complete moment (U7)", async () => {
    const api = fast();
    renderApp("/builder", { api, openCharacterId: await completeSeeded(api) });
    const status = await screen.findByRole("contentinfo", { name: "Status" });
    await waitFor(() => expect(within(status).getByRole("status").textContent).not.toMatch(/^Last checked/));
    expect(screen.queryByRole("group", { name: "Character complete" })).toBeNull();
  });

  // B-139 (tester video C4): a running command put a 150 px bar inline, so the pending prompt wrapped one word per line.
  it("on a phone a running command shows a thin line on the footer's edge, not a bar in the message row", async () => {
    setPhoneForTests(true);
    const api = fast();
    let finish: () => void = () => undefined;
    const real = api.spendResource.bind(api);
    vi.spyOn(api, "spendResource").mockImplementation((a) => new Promise((res) => { finish = () => res(real(a)); }));
    const user = userEvent.setup();
    renderApp("/play", { api, openCharacterId: await seededId(api, "Seraphine") });
    const status = await screen.findByRole("contentinfo", { name: "Status" });
    await user.click(await screen.findByRole("button", { name: "Spend Rage" }));
    expect(within(status).queryByRole("progressbar"), "a quick command shows nothing").toBeNull();
    const bar = await within(status).findByRole("progressbar", { name: "Working" });
    expect(bar.classList.contains("aur-status-busyline")).toBe(true);
    expect(bar.style.width, "no fixed width that squeezes the message").toBe("");
    const flow = [...status.children].filter((c) => !c.classList.contains("aur-status-busyline")).map((c) => c.className);
    expect(flow.every((c) => !c.includes("aur-progress"))).toBe(true);
    await act(async () => { finish(); });
    await waitFor(() => expect(within(status).queryByRole("progressbar")).toBeNull());
  });

  it("a narrow desktop window keeps the inline status-bar progress bar (U7)", async () => {
    const api = fast();
    let finish: () => void = () => undefined;
    const real = api.spendResource.bind(api);
    vi.spyOn(api, "spendResource").mockImplementation((a) => new Promise((res) => { finish = () => res(real(a)); }));
    const user = userEvent.setup();
    renderApp("/play", { api, openCharacterId: await seededId(api, "Seraphine") });
    const status = await screen.findByRole("contentinfo", { name: "Status" });
    await user.click(await screen.findByRole("button", { name: "Spend Rage" }));
    const bar = within(status).getByRole("progressbar", { name: "Working" });
    expect(bar.classList.contains("aur-status-busyline")).toBe(false);
    expect(bar.style.width).toBe("150px");
    await act(async () => { finish(); });
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

