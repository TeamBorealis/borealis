import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { AppProviders } from "../AppProviders";
import type { ClassSpellcasting, Page, SelectionSlot, SlotOption } from "../api/contract";
import { MockApi } from "../api/mock";
import { mockEditionMarks, USED_AS_WRITTEN } from "../api/mock/edition-marks";
import { badgeTitle, convertedTitle, editionMarks } from "../components/EditionMarks";
import { PickerGroup } from "../pages/builder/PickerGroup";
import { SlotOptionsGrid } from "../pages/spells/SlotOptionsGrid";

/** R6 / R6a (B-18) and B-55 sec. 1.3: picker edition mark " (2014)" after the source, "converted" tag and adapter notes. */
const NOTE = "Subclass features listed below level 3 arrive at level 3";

function option(id: string, name: string, extra: Partial<SlotOption> = {}): SlotOption {
  return {
    id, name, type: "archetype", source: "Test", sourceName: "Test Book", shortDescription: "", prerequisites: null,
    rulesets: [extra.edition ?? "dnd2024"], ...extra,
  } as SlotOption;
}

const ROWS: SlotOption[] = [
  option("ID_T_NATIVE", "Tidewarden"),
  option("ID_T_ASWRITTEN", "Glass Choir", { edition: "dnd2014", editionNotes: [USED_AS_WRITTEN] }),
  option("ID_T_CONVERTED", "Old Lantern Way", { edition: "dnd2014", converted: true, editionNotes: [NOTE] }),
];

function apiWith(rows: SlotOption[]): MockApi {
  const api = new MockApi({ aiChunkDelayMs: 1 });
  api.presetSettings({ showImportNotice: false });
  const page: Page<SlotOption> = { total: rows.length, items: rows };
  api.getSlotOptions = async () => page;
  return api;
}

const SLOT = { id: "s1", title: "Subclass", groupLabel: null, locked: false, chosen: null, elementType: "archetype", hint: null, requiredLevel: 3 } as unknown as SelectionSlot;

function renderPicker(rows: SlotOption[]) {
  render(
    <AppProviders api={apiWith(rows)}>
      <MemoryRouter>
        <PickerGroup characterId="c1" group={{ key: "s1", label: null, title: "Subclass", slots: [SLOT] }} isNext={false} open
          onToggle={() => {}} describedId={null} onDescribe={() => {}} onChoose={() => {}} onClear={() => {}} />
      </MemoryRouter>
    </AppProviders>,
  );
}

describe("edition marks rule", () => {
  it("marks nothing on a same-edition or edition-neutral option", () => {
    expect(editionMarks({})).toEqual({ badge: null, converted: false, notes: [] });
    // converted without an edition is not a foreign option: no tag either
    expect(editionMarks({ converted: true, editionNotes: [NOTE] }).converted).toBe(false);
  });

  it("badges an other-edition option with the year it was written for", () => {
    expect(editionMarks({ edition: "dnd2014" }).badge).toBe("2014");
    expect(editionMarks({ edition: "dnd2024" }).badge).toBe("2024");
  });

  it("tags only adapter-converted options and keeps the adapter's notes", () => {
    const asWritten = editionMarks({ edition: "dnd2014", editionNotes: [USED_AS_WRITTEN] });
    expect(asWritten.converted).toBe(false);
    expect(badgeTitle(asWritten)).toBe("Written for the 2014 rules; used as written");
    expect(convertedTitle(asWritten)).toBe("");
    const conv = editionMarks({ edition: "dnd2014", converted: true, editionNotes: [NOTE] });
    expect(conv).toEqual({ badge: "2014", converted: true, notes: [NOTE] });
    expect(badgeTitle(conv)).toBe("Written for the 2014 rules");
    expect(convertedTitle(conv)).toBe(`Converted from the 2014 rules: ${NOTE}`);
  });
});

describe("mock adapter stand-in", () => {
  it("marks only options written for the other edition", () => {
    expect(mockEditionMarks({ id: "x", rulesets: ["dnd2014", "dnd2024"] }, "dnd2024")).toEqual({});
    expect(mockEditionMarks({ id: "x", rulesets: ["dnd2024"] }, "dnd2024")).toEqual({});
    expect(mockEditionMarks({ id: "shadow-blade", rulesets: ["dnd2024"] }, "dnd2014")).toEqual({ edition: "dnd2024", editionNotes: [USED_AS_WRITTEN] });
    expect(mockEditionMarks({ id: "light-domain", rulesets: ["dnd2014"] }, "dnd2024")).toEqual({ edition: "dnd2014", converted: true, editionNotes: [NOTE] });
  });
});

describe("builder picker edition marks", () => {
  it("marks only other-edition rows (' (2014)' after the source) and tags only converted rows", async () => {
    renderPicker(ROWS);
    const list = await screen.findByRole("list", { name: "Subclass options" });
    const items = within(list).getAllByRole("listitem");
    const byName = (n: string) => items.find((li) => li.textContent?.includes(n))!;
    expect(byName("Tidewarden").querySelector(".ed-mark")).toBeNull();
    expect(byName("Tidewarden").querySelector(".ed-tag")).toBeNull();
    expect(byName("Glass Choir").querySelector(".pick-src-name")?.textContent).toBe("Test Book (2014)");
    expect(byName("Glass Choir").querySelector(".ed-mark")?.getAttribute("title")).toBe("Written for the 2014 rules; used as written");
    expect(byName("Glass Choir").querySelector(".ed-tag")).toBeNull();
    expect(byName("Old Lantern Way").querySelector(".ed-mark")?.textContent).toBe(" (2014)");
    expect(document.querySelector(".ed-badge")).toBeNull();
    expect(byName("Old Lantern Way").querySelector(".ed-tag")).not.toBeNull();
  });

  it("the converted tag carries the notes on hover and toggles them inline on tap", async () => {
    renderPicker(ROWS);
    const tag = await screen.findByRole("button", { name: `Converted from the 2014 rules: ${NOTE}` });
    expect(tag.getAttribute("title")).toBe(`Converted from the 2014 rules: ${NOTE}`);
    expect(tag.getAttribute("aria-expanded")).toBe("false");
    const notes = document.getElementById(tag.getAttribute("aria-controls")!)!;
    expect(notes.hidden).toBe(true);
    fireEvent.click(tag);
    expect(tag.getAttribute("aria-expanded")).toBe("true");
    expect(notes.hidden).toBe(false);
    expect(notes.textContent).toBe(`Converted from the 2014 rules: ${NOTE}`);
    fireEvent.click(tag);
    expect(notes.hidden).toBe(true);
  });

  it("tapping the tag does not describe or choose the option", async () => {
    let described = 0;
    let chosen = 0;
    render(
      <AppProviders api={apiWith(ROWS)}>
        <MemoryRouter>
          <PickerGroup characterId="c1" group={{ key: "s1", label: null, title: "Subclass", slots: [SLOT] }} isNext={false} open
            onToggle={() => {}} describedId={null} onDescribe={() => { described += 1; }} onChoose={() => { chosen += 1; }} onClear={() => {}} />
        </MemoryRouter>
      </AppProviders>,
    );
    const tag = await screen.findByRole("button", { name: /^Converted from/ });
    fireEvent.click(tag);
    fireEvent.doubleClick(tag);
    expect(described).toBe(0);
    expect(chosen).toBe(0);
  });
});

describe("spell slot picker edition marks", () => {
  it("shows the edition mark and converted tag in the source column", async () => {
    const spells = [
      option("ID_SP_NATIVE", "Ember Mote", { type: "spell", level: 0 }),
      option("ID_SP_OLD", "Lantern Hymn", { type: "spell", level: 0, edition: "dnd2014", converted: true, editionNotes: ["Range changed"] }),
    ];
    const cls = { className: "Wizard" } as ClassSpellcasting;
    render(
      <AppProviders api={apiWith(spells)}>
        <MemoryRouter>
          <SlotOptionsGrid characterId="c1" cls={cls} slot={{ ...SLOT, elementType: "spell" } as SelectionSlot} picked={null} onPick={() => {}} onChoose={() => {}} />
        </MemoryRouter>
      </AppProviders>,
    );
    const table = await screen.findByRole("table", { name: "Wizard spells" });
    const old = (await within(table).findByText("Lantern Hymn")).closest("tr")!;
    expect(old.querySelector(".sp-src .sp-src-name")?.textContent).toBe("Test Book (2014)");
    expect(old.querySelector(".ed-badge")).toBeNull();
    expect(within(old).getByRole("button", { name: "Converted from the 2014 rules: Range changed" })).toBeTruthy();
    const native = within(table).getByText("Ember Mote").closest("tr")!;
    expect(native.querySelector(".ed-mark")).toBeNull();
    expect(native.querySelector(".sp-src")?.textContent).toBe("Test Book");
  });
});

describe("mock builder end to end", () => {
  it("a 2024 cleric's subclass picker marks the 2014-only subclass as converted", async () => {
    const api = new MockApi({ aiChunkDelayMs: 1 });
    const c = await api.createCharacter({ request: { name: "R6", ruleset: "dnd2024", startingLevel: 3, gender: "", abilityMethod: "point_buy", options: {} } });
    const classSlot = c.slots.find((s) => s.elementType === "class")!;
    const after = await api.applySelection({ characterId: c.id, slotId: classSlot.id, elementId: "cleric" });
    const sub = after.slots.find((s) => s.elementType === "archetype");
    expect(sub).toBeDefined();
    const page = await api.getSlotOptions({ characterId: c.id, slotId: sub!.id });
    const light = page.items.find((o) => o.id === "light-domain")!;    expect(light).toMatchObject({ edition: "dnd2014", converted: true, editionNotes: [NOTE] });
    const life = page.items.find((o) => o.id === "life-domain")!;
    expect(life.edition).toBeUndefined();
  });
});
