import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";
import { AppProviders } from "../AppProviders";
import type { Page, SelectionSlot, SlotOption, SlotOptionBase } from "../api/contract";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { speciesTerm } from "../lib/terminology";
import { groupSlots, PickerGroup } from "../pages/builder/PickerGroup";

/**
 * A15 (tester video 2026-11-02, B-126): the host names the species slot after the engine's select ("Race"), while the tab
 * uses the ruleset's word. A 2024 character's picker reads "Species" and its search "Search species"; 2014 keeps "Race".
 */
afterEach(() => setPhoneForTests(false));

const RACE_SLOT = {
  id: "race-slot", title: "Race", groupLabel: null, locked: false, chosen: null, elementType: "race", requiredLevel: null, hint: null,
} as unknown as SelectionSlot;

const race = (i: number): SlotOption => ({
  id: `r${i}`, name: `Folk ${i}`, type: "race", source: "SRD 5.2.1", sourceName: "SRD 5.2.1", shortDescription: "", prerequisites: null, rulesets: ["dnd2024"],
} as SlotOptionBase as SlotOption);
// Nine rows: more than eight, so the filter bar (and its search) shows.
const ROWS = Array.from({ length: 9 }, (_, i) => race(i + 1));

function renderRacePicker() {
  const api = new MockApi({ aiChunkDelayMs: 1 });
  api.presetSettings({ showImportNotice: false });
  api.getSlotOptions = async (): Promise<Page<SlotOption>> => ({ total: ROWS.length, items: ROWS });
  const [group] = groupSlots([RACE_SLOT], "dnd2024");
  render(
    <AppProviders api={api}>
      <MemoryRouter>
        <PickerGroup characterId="c1" group={group!} isNext={false} open
          onToggle={() => {}} describedId={null} onDescribe={() => {}} onChoose={() => {}} onClear={() => {}} />
      </MemoryRouter>
    </AppProviders>,
  );
}

describe("the species picker follows the ruleset's word (A15)", () => {
  it("a 2024 character's picker is titled Species, with Search species", async () => {
    setPhoneForTests(true);
    renderRacePicker();
    expect(await screen.findByPlaceholderText("Search species")).toBeTruthy();
    expect(screen.getByRole("list", { name: "Species options" })).toBeTruthy();
    expect(screen.queryByText("Race")).toBeNull();
    expect(screen.getByText("Species")).toBeTruthy();
  });

  it("desktop keeps the classic capitals: SPECIES", async () => {
    renderRacePicker();
    expect(await screen.findByPlaceholderText("Search species")).toBeTruthy();
    expect(screen.getByText("SPECIES")).toBeTruthy();
  });

  it("a 2014 character keeps Race, and other slots keep their own titles", () => {
    expect(groupSlots([RACE_SLOT], "dnd2014")[0]!.title).toBe("Race");
    const subrace = { ...RACE_SLOT, id: "sub", title: "Subrace", elementType: "subrace" } as SelectionSlot;
    expect(groupSlots([subrace], "dnd2024")[0]!.title).toBe("Subrace");
    expect(speciesTerm("dnd2024")).toBe("Species");
    expect(speciesTerm("dnd2014")).toBe("Race");
  });
});
