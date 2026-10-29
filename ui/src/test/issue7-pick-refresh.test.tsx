import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Page, Ruleset, SlotOption, SlotOptionsArgs } from "../api/contract";
import { MockApi } from "../api/mock";
import { samePage } from "../pages/builder/usePickerRows";
import { renderApp } from "./utils";

/**
 * GitHub #7: a pick can change what the other open lists of the tab offer (a skill taken by one "Rogue skill" list leaves the
 * others; a homebrew skill-tree node unlocks its child in the sibling list). The open lists show the change at once, without
 * collapsing, and keep their filter text, their rows (no remount, so focus and scroll stay) and the focus.
 */

const TITLE = "Rogue skill";
const BRANCH = "Branch of Athletics";

async function rogue(api: MockApi, ruleset: Ruleset): Promise<{ id: string; skillSlots: string[] }> {
  const created = await api.createCharacter({ request: { name: "Tree", ruleset, startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
  const classSlot = created.slots.find((s) => s.elementType === "class")!;
  const snap = await api.applySelection({ characterId: created.id, slotId: classSlot.id, elementId: "rogue" });
  const skillSlots = snap.slots.filter((s) => s.title === TITLE).map((s) => s.id);
  expect(skillSlots.length).toBe(4);
  return { id: snap.id, skillSlots };
}

const listOf = (slotId: string): HTMLUListElement | null => document.querySelector<HTMLUListElement>(`.pick[data-slot-id="${slotId}"] ul.pick-list`);
const rowIn = (slotId: string, name: string): HTMLLIElement | null => listOf(slotId)?.querySelector<HTMLLIElement>(`li[data-row="${name}"]`) ?? null;
const rowNames = (slotId: string): string[] => [...(listOf(slotId)?.querySelectorAll("li[data-row]") ?? [])].map((li) => li.getAttribute("data-row")!);

/** Records the fewest rows a list held while it changed (0 would be a flash of an empty list). */
function watchRows(list: HTMLUListElement): { min: () => number; stop: () => void } {
  let min = list.querySelectorAll("li[data-row]").length;
  const obs = new MutationObserver(() => { min = Math.min(min, list.querySelectorAll("li[data-row]").length); });
  obs.observe(list, { childList: true });
  return { min: () => min, stop: () => obs.disconnect() };
}

describe("#7 reloads that change nothing", () => {
  it("samePage tells an equal page from a changed one", () => {
    const row = (id: string, extra: Partial<SlotOption> = {}): SlotOption =>
      ({ id, name: id, type: "skill", source: "core", shortDescription: "", prerequisites: null, rulesets: ["dnd2024"], ...extra }) as SlotOption;
    const page = (...items: SlotOption[]): Page<SlotOption> => ({ total: items.length, items });
    expect(samePage(page(row("a"), row("b")), page(row("a"), row("b")))).toBe(true);
    expect(samePage(page(row("a")), page(row("a"), row("b")))).toBe(false);
    expect(samePage(page(row("a")), page(row("a", { disabledReason: "Already chosen" })))).toBe(false);
    expect(samePage(null, page())).toBe(false);
  });
});

describe.each<Ruleset>(["dnd2014", "dnd2024"])("#7 open pick lists follow every pick (%s)", (ruleset) => {
  it("a skill picked in one list leaves the open sibling list at once, keeping its filter, rows and focus", async () => {
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const { id, skillSlots } = await rogue(api, ruleset);
    const [first, second] = [skillSlots[0]!, skillSlots[1]!];
    const user = userEvent.setup();
    renderApp("/builder/proficiencies", { api, openCharacterId: id });
    await waitFor(() => expect(rowIn(second, "Athletics")).not.toBeNull());
    expect(rowIn(first, "Athletics")).not.toBeNull();

    // the second list's filter bar has text and the focus
    const pick2 = document.querySelector<HTMLElement>(`.pick[data-slot-id="${second}"]`)!;
    const search = pick2.querySelector<HTMLInputElement>("input.pick-fsearch")!;
    await user.type(search, "th");
    await waitFor(() => expect(rowNames(second)).not.toContain("Acrobatics"));
    expect(document.activeElement).toBe(search);
    const list2 = listOf(second)!;
    const stealth = rowIn(second, "Stealth")!;
    const watch = watchRows(list2);

    // a double click chooses (it does not move the focus)
    fireEvent.doubleClick(rowIn(first, "Athletics")!.querySelector("button.pick-main")!);

    await waitFor(() => expect(rowNames(second)).not.toContain("Athletics"));
    watch.stop();
    expect(document.querySelector(`.pick-chosen[data-slot-id="${first}"]`)?.textContent).toContain("Athletics");
    expect(listOf(second)).toBe(list2); // still open, never remounted
    expect(rowIn(second, "Stealth")).toBe(stealth);
    expect(watch.min()).toBeGreaterThan(0);
    expect(search.value).toBe("th");
    expect(document.activeElement).toBe(search);
    // the third and fourth lists follow too
    expect(rowNames(skillSlots[2]!)).not.toContain("Athletics");
    expect(rowNames(skillSlots[3]!)).not.toContain("Athletics");
  });

  it("an option a pick unlocks appears in the open sibling list, and goes when the pick is cleared", async () => {
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const { id, skillSlots } = await rogue(api, ruleset);
    const [first, second] = [skillSlots[0]!, skillSlots[1]!];
    // a skill-tree node: offered to the other skill lists once Athletics is held
    const base = api.getSlotOptions.bind(api);
    api.getSlotOptions = async (a: SlotOptionsArgs): Promise<Page<SlotOption>> => {
      const page = await base(a);
      const snap = await api.getSnapshot({ characterId: a.characterId });
      const held = snap.slots.some((s) => s.title === TITLE && s.chosen?.id === "skill-athletics");
      const own = snap.slots.find((s) => s.id === a.slotId);
      if (!held || own?.chosen?.id === "skill-athletics" || !skillSlots.includes(a.slotId)) return page;
      const node: SlotOption = { ...page.items[0]!, id: "branch-athletics", name: BRANCH, versions: undefined };
      return { total: page.total + 1, items: [...page.items, node] };
    };
    renderApp("/builder/proficiencies", { api, openCharacterId: id });
    await waitFor(() => expect(rowIn(second, "Athletics")).not.toBeNull());
    expect(rowIn(second, BRANCH)).toBeNull();
    const list2 = listOf(second)!;

    fireEvent.doubleClick(rowIn(first, "Athletics")!.querySelector("button.pick-main")!);
    await waitFor(() => expect(rowIn(second, BRANCH)).not.toBeNull());
    expect(listOf(second)).toBe(list2);

    // the existing removal behaviour: clearing the pick takes the unlocked option away again
    fireEvent.click(await screen.findByRole("button", { name: "Clear Athletics" }));
    await waitFor(() => expect(rowIn(second, BRANCH)).toBeNull());
    expect(rowIn(second, "Athletics")).not.toBeNull();
    expect(listOf(second)).toBe(list2);
  });
});
