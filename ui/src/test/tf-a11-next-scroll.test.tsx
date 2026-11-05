import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CharacterSnapshot } from "../api/contract";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { renderApp } from "./utils";

/**
 * A11 (tester video 2026-11-02): Next selection switched to the target's tab but kept the old scroll offset, so on a long tab
 * the target picker was off screen. Aurora "scrolls it into view" (ui-inventory 3.6); a plain sub-tab tap starts at the top.
 */
const SCROLLER_TOP = 100;
/** Where the target picker sits in the scrolled content (px below the content's top). */
const TARGET_OFFSET = 1500;
/** The target header must land within this many px of the scroller's top edge. */
const NEAR_TOP = 120;

async function raceAndClass(api: MockApi): Promise<CharacterSnapshot> {
  const created = await api.createCharacter({ request: { name: "Torrin", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
  const race = created.slots.find((s) => s.elementType === "race")!;
  const withRace = await api.applySelection({ characterId: created.id, slotId: race.id, elementId: "human" });
  const cls = withRace.slots.find((s) => s.elementType === "class")!;
  let snap = await api.applySelection({ characterId: created.id, slotId: cls.id, elementId: "fighter" });
  // The Fighter's own choices (fighting style, weapon mastery, ...) first: Next selection then points at Background.
  for (let i = 0; i < 20; i++) {
    const next = snap.slots.find((s) => s.id === snap.progress.nextSlotId);
    if (!next || next.tab !== "race" && next.tab !== "class") break;
    const options = await api.getSlotOptions({ characterId: snap.id, slotId: next.id, query: "" });
    const held = new Set(snap.slots.map((s) => s.chosen?.id));
    const pick = options.items.find((o) => !o.disabledReason && !held.has(o.id))!;
    snap = await api.applySelection({ characterId: snap.id, slotId: next.id, elementId: pick.id });
  }
  return snap;
}

/** Gives main.aur-content a scroll offset and lays the target picker out TARGET_OFFSET px down its content. */
function fakeLayout(targetSlotId: () => string | null) {
  const scroller = document.querySelector<HTMLElement>("main.aur-content")!;
  let top = 600; // scrolled down the previous tab
  Object.defineProperty(scroller, "scrollTop", { configurable: true, get: () => top, set: (v: number) => { top = v; } });
  const rect = (y: number) => ({ top: y, bottom: y + 30, height: 30, left: 0, right: 400, width: 400, x: 0, y, toJSON: () => ({}) });
  const real = Element.prototype.getBoundingClientRect;
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    if (this === scroller) return rect(SCROLLER_TOP) as DOMRect;
    if (this.classList.contains("pick") && this.getAttribute("data-slot-id") === targetSlotId()) return rect(SCROLLER_TOP + TARGET_OFFSET - top) as DOMRect;
    return real.call(this);
  });
  return { scrollTop: () => top, headerTop: () => SCROLLER_TOP + TARGET_OFFSET - top };
}

afterEach(() => { vi.restoreAllMocks(); setPhoneForTests(false); });

describe("Next selection scrolls to its picker (A11)", () => {
  it.each([["desktop", false], ["phone", true]])("brings the target header to the top of the page (%s)", async (_name, phone) => {
    setPhoneForTests(phone);
    const user = userEvent.setup();
    const api = new MockApi();
    const snap = await raceAndClass(api);
    const target = snap.slots.find((s) => s.id === snap.progress.nextSlotId)!;
    expect(target.tab).toBe("background");
    renderApp("/builder/class", { api, openCharacterId: snap.id });
    await screen.findByRole("tab", { name: "Class", selected: true });
    const layout = fakeLayout(() => target.id);

    await user.click(await screen.findByRole("button", { name: "Next selection" }));
    await screen.findByRole("tab", { name: "Background", selected: true });
    await waitFor(() => {
      const landed = layout.headerTop() - SCROLLER_TOP;
      expect(landed).toBeGreaterThanOrEqual(0);
      expect(landed).toBeLessThanOrEqual(NEAR_TOP);
    });
  });

  it("a plain sub-tab tap starts at the top", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const snap = await raceAndClass(api);
    renderApp("/builder/class", { api, openCharacterId: snap.id });
    await screen.findByRole("tab", { name: "Class", selected: true });
    const layout = fakeLayout(() => null);
    expect(layout.scrollTop()).toBe(600);
    await user.click(screen.getByRole("tab", { name: "Languages" }));
    await screen.findByRole("tab", { name: "Languages", selected: true });
    expect(layout.scrollTop()).toBe(0);
  });
});
