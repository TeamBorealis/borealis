import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AppProviders } from "../AppProviders";
import type { Page, SelectionSlot, SlotOption, SlotOptionBase } from "../api/contract";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { PickerGroup } from "../pages/builder/PickerGroup";
import { renderApp } from "./utils";

/**
 * A1 (tester video 2026-11-02, B-112) with TV1: the empty ring on a phone picker row looked like a radio button, but a tap
 * only described the row. On phones the ring is now a real 44 pt button that chooses the row; a tap on the name still only
 * describes it, the Choose button stays, and desktop keeps Aurora's rows (U7).
 */
afterEach(() => setPhoneForTests(false));

const SLOT = { id: "sp", title: "Species", groupLabel: null, locked: false, chosen: null, elementType: "race", requiredLevel: null, hint: null } as unknown as SelectionSlot;
const opt = (id: string, name: string, extra: Partial<SlotOptionBase> = {}): SlotOption => ({
  id, name, type: "race", source: "SRD 5.2.1", sourceName: "SRD 5.2.1", shortDescription: "", prerequisites: null, rulesets: ["dnd2024"], ...extra,
} as SlotOptionBase as SlotOption);
const ROWS = [opt("dwarf", "Dwarf"), opt("elf", "Elf"), opt("orc", "Orc", { disabledReason: "Not allowed here" })];

function renderPicker() {
  const onChoose = vi.fn();
  const onDescribe = vi.fn();
  const api = new MockApi({ aiChunkDelayMs: 1 });
  api.presetSettings({ showImportNotice: false });
  api.getSlotOptions = async (): Promise<Page<SlotOption>> => ({ total: ROWS.length, items: ROWS });
  render(
    <AppProviders api={api}>
      <MemoryRouter>
        <PickerGroup characterId="c1" group={{ key: SLOT.id, label: null, title: SLOT.title, slots: [SLOT] }} isNext={false} open
          onToggle={() => {}} describedId={null} onDescribe={onDescribe} onChoose={onChoose} onClear={() => {}} />
      </MemoryRouter>
    </AppProviders>,
  );
  return { onChoose, onDescribe };
}

const rowOf = async (name: string) => {
  const list = await screen.findByRole("list", { name: "Species options" });
  await waitFor(() => expect(within(list).getAllByRole("listitem").length).toBe(ROWS.length));
  return within(list).getAllByRole("listitem").find((li) => li.getAttribute("data-row") === name)!;
};

describe("the phone picker ring (A1)", () => {
  it("tapping the ring of a picker row chooses it on phones", async () => {
    setPhoneForTests(true);
    const { onChoose } = renderPicker();
    const dwarf = await rowOf("Dwarf");
    await userEvent.setup().click(within(dwarf).getByRole("button", { name: "Choose Dwarf" }));
    expect(onChoose).toHaveBeenCalledWith("sp", "dwarf");
  });

  it("tapping a row name still only describes it", async () => {
    setPhoneForTests(true);
    const { onChoose, onDescribe } = renderPicker();
    const dwarf = await rowOf("Dwarf");
    await userEvent.setup().click(dwarf.querySelector<HTMLButtonElement>(".pick-main")!);
    expect(onDescribe).toHaveBeenCalled();
    expect(onChoose).not.toHaveBeenCalled();
  });

  it("a disabled row's ring is aria-disabled and does nothing", async () => {
    setPhoneForTests(true);
    const { onChoose } = renderPicker();
    const ring = within(await rowOf("Orc")).getByRole("button", { name: "Choose Orc" });
    expect(ring.getAttribute("aria-disabled")).toBe("true");
    await userEvent.setup().click(ring);
    expect(onChoose).not.toHaveBeenCalled();
  });

  it("the ring is a 44 pt target drawn by the button, no longer by the row", async () => {
    const css = (await import("./cssRules")).readCss("styles/phone/lists.css");
    expect(css).toMatch(/html\.phone \.pick-mark \{[^}]*width: var\(--ph-hit\)[^}]*height: var\(--ph-hit\)/);
    expect(css).not.toMatch(/\.pick-main::after/);
  });

  it("desktop rows have no ring button (U7)", async () => {
    renderPicker();
    expect(within(await rowOf("Dwarf")).queryByRole("button", { name: "Choose Dwarf" })).toBeNull();
  });

  it("in the app the ring applies the pick: the row is pressed and the footer names it", async () => {
    setPhoneForTests(true);
    const api = new MockApi({ aiChunkDelayMs: 1 });
    const snap = await api.createCharacter({ request: { name: "Ring", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
    const spy = vi.spyOn(api, "applySelection");
    renderApp("/builder/race", { api, openCharacterId: snap.id });
    const list = await screen.findByRole("list", { name: "Species options" });
    const ring = await within(list).findByRole("button", { name: "Choose Dwarf" });
    await userEvent.setup().click(ring);
    await waitFor(() => expect(spy).toHaveBeenCalledWith(expect.objectContaining({ elementId: "dwarf" })));
    const footer = await waitFor(() => {
      const chosen = document.querySelector(".pick-chosen");
      expect(chosen?.textContent).toContain("Dwarf");
      return chosen;
    });
    expect(footer).not.toBeNull();
  });
});
