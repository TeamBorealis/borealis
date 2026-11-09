import { render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";
import { AppProviders } from "../AppProviders";
import type { Page, SelectionSlot, SlotOption, SlotOptionBase } from "../api/contract";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { PickerGroup } from "../pages/builder/PickerGroup";

/**
 * A12 (tester video 2026-11-02, B-123) with TV2: on phones a list whose rows all share one source drops the per-row source
 * line ("SRD 5.2.1" on every row); a list with several sources keeps it, and desktop keeps it always (U7).
 */
afterEach(() => setPhoneForTests(false));

const SLOT = { id: "bg", title: "Background", groupLabel: null, locked: false, chosen: null, elementType: "background", requiredLevel: null, hint: null } as unknown as SelectionSlot;

const row = (id: string, name: string, sourceName = "SRD 5.2.1"): SlotOption => ({
  id, name, type: "background", source: sourceName, sourceName, shortDescription: "", prerequisites: null, rulesets: ["dnd2024"],
} as SlotOptionBase as SlotOption);

const ONE_SOURCE = [row("a", "Acolyte"), row("b", "Criminal"), row("c", "Sage")];

function renderPicker(rows: SlotOption[]) {
  const api = new MockApi({ aiChunkDelayMs: 1 });
  api.presetSettings({ showImportNotice: false });
  api.getSlotOptions = async (): Promise<Page<SlotOption>> => ({ total: rows.length, items: rows });
  render(
    <AppProviders api={api}>
      <MemoryRouter>
        <PickerGroup characterId="c1" group={{ key: SLOT.id, label: null, title: SLOT.title, slots: [SLOT] }} isNext={false} open
          onToggle={() => {}} describedId={null} onDescribe={() => {}} onChoose={() => {}} onClear={() => {}} />
      </MemoryRouter>
    </AppProviders>,
  );
}

async function rowsShown(): Promise<HTMLElement[]> {
  const list = await screen.findByRole("list", { name: "Background options" });
  await waitFor(() => expect(within(list).getAllByRole("listitem").length).toBeGreaterThan(0));
  return within(list).getAllByRole("listitem");
}

describe("the per-row source line on phones (A12)", () => {
  it("a single-source list has no source text on its rows, and the row's name still says it", async () => {
    setPhoneForTests(true);
    renderPicker(ONE_SOURCE);
    const rows = await rowsShown();
    for (const li of rows) expect(li.querySelector(".pick-src")?.textContent ?? "").toBe("");
    expect(within(rows[0]!).getByRole("button", { name: "Acolyte, SRD 5.2.1" })).toBeTruthy();
  });

  it("a mixed list keeps every row's source", async () => {
    setPhoneForTests(true);
    renderPicker([...ONE_SOURCE, row("d", "Wanderer", "Sample Expansion")]);
    const rows = await rowsShown();
    expect(rows.map((li) => li.querySelector(".pick-src")?.textContent)).toEqual(["SRD 5.2.1", "SRD 5.2.1", "SRD 5.2.1", "Sample Expansion"]);
  });

  it("desktop keeps the source on every row", async () => {
    renderPicker(ONE_SOURCE);
    const rows = await rowsShown();
    for (const li of rows) expect(li.querySelector(".pick-src")?.textContent).toBe("SRD 5.2.1");
  });
});
