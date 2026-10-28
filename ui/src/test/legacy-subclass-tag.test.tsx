import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { AppProviders } from "../AppProviders";
import type { Page, SelectionSlot, SlotOption } from "../api/contract";
import { MockApi } from "../api/mock";
import { editionSuffix } from "../lib/versions";
import { PickerGroup } from "../pages/builder/PickerGroup";

/** B-44: a 2014 subclass offered to a 2024 class carries an edition tag after its source. */
function option(id: string, name: string, edition?: SlotOption["edition"]): SlotOption {
  return {
    id, name, type: "archetype", source: "Test", sourceName: "Test Book", shortDescription: "", prerequisites: null,
    rulesets: [edition ?? "dnd2024"], ...(edition ? { edition } : {}),
  } as SlotOption;
}

describe("Legacy subclass edition tag (B-44)", () => {
  it("editionSuffix writes the edition the way the description pane does, unless the source names the year", () => {
    expect(editionSuffix(undefined, "Test Book")).toBe("");
    expect(editionSuffix("dnd2014", "Test Book")).toBe(" (2014)");
    expect(editionSuffix("dnd2024", "Test Book")).toBe(" (2024)");
    expect(editionSuffix("dnd2014", "Old Guide (2014)")).toBe("");
  });

  it("the subclass picker tags only the other-edition option", async () => {
    const api = new MockApi({ aiChunkDelayMs: 1 });
    api.presetSettings({ showImportNotice: false });
    const page: Page<SlotOption> = { total: 2, items: [option("ID_T_NEW", "Tidewarden"), option("ID_T_OLD", "Old Lantern Way", "dnd2014")] };
    api.getSlotOptions = async () => page;
    const slot = { id: "s1", title: "Subclass", groupLabel: null, locked: false, chosen: null, elementType: "archetype", hint: null, requiredLevel: 3 } as unknown as SelectionSlot;
    render(
      <AppProviders api={api}>
        <MemoryRouter>
          <PickerGroup characterId="c1" group={{ key: "s1", label: null, title: "Subclass", slots: [slot] }} isNext={false} open
            onToggle={() => {}} describedId={null} onDescribe={() => {}} onChoose={() => {}} onClear={() => {}} />
        </MemoryRouter>
      </AppProviders>,
    );
    const list = await screen.findByRole("list", { name: "Subclass options" });
    const legacy = await within(list).findByRole("button", { name: "Old Lantern Way, Test Book, 2014 rules" });
    // B-55 sec. 1.3: the edition is " (2014)" after the source name, in the source's own font (no boxed badge).
    expect(legacy.querySelector(".pick-src-name")?.textContent).toBe("Test Book (2014)");
    expect(legacy.querySelector(".ed-badge")).toBeNull();
    const native = within(list).getByRole("button", { name: "Tidewarden, Test Book" });
    expect(native.querySelector(".pick-src")?.textContent).toBe("Test Book");
    expect(native.querySelector(".ed-badge")).toBeNull();
  });
});
