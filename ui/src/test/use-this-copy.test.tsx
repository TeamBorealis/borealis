import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, it, vi } from "vitest";
import { AppProviders } from "../AppProviders";
import type { ElementDetail, ElementSummary, Page, SelectionSlot, SlotOption, SlotOptionBase, SlotOptionsArgs } from "../api/contract";
import { MockApi } from "../api/mock";
import { copyKey, pickShown, visibleRows, buildRows, NO_FILTER } from "../lib/versions";
import { PickerGroup } from "../pages/builder/PickerGroup";
import { CompendiumBrowser, copyEditions } from "../pages/compendium/CompendiumFlyout";

/** GitHub #51 piece 1: a remembered "Always use this copy" per name and edition, used as the default copy. */

type V = SlotOptionBase;
const ver = (id: string, sourceName: string, rulesets: V["rulesets"], extra: Partial<V> = {}): V => ({
  id, name: "Fireball", type: "spell", source: sourceName.toLowerCase().replace(/\W+/g, "-"), sourceName, shortDescription: `Fireball from ${sourceName}.`,
  prerequisites: null, rulesets, level: 3, ...extra,
});
const FB_SRD = ver("fb-srd", "SRD 5.2.1", ["dnd2024"], { bundled: true });
const FB_HOME = ver("fb-home", "Table Homebrew", ["dnd2024"]);
const FB_OLD = ver("fb-old", "SRD 5.1", ["dnd2014"], { bundled: true, edition: "dnd2014" });
const FIREBALL: SlotOption = { ...FB_HOME, versions: [FB_HOME, FB_SRD, FB_OLD] };
const KEY_2024 = "spell:dnd2024:fireball";

const SLOT = { id: "s1", title: "Spell", groupLabel: null, locked: false, chosen: null, elementType: "spell", hint: null, requiredLevel: null } as unknown as SelectionSlot;

function apiWith(rows: SlotOption[], preferredCopies: Record<string, string> = {}) {
  const api = new MockApi({ aiChunkDelayMs: 1 });
  api.presetSettings({ showImportNotice: false, preferredCopies });
  api.getSlotOptions = async (a: SlotOptionsArgs): Promise<Page<SlotOption>> => ({ total: rows.length, items: rows.slice(a.offset ?? 0) });
  return api;
}

function renderPicker(api: MockApi, slot: SelectionSlot = SLOT) {
  return render(
    <AppProviders api={api}>
      <MemoryRouter>
        <PickerGroup characterId="c1" group={{ key: slot.id, label: null, title: slot.title, slots: [slot] }} isNext={false} open
          onToggle={() => {}} describedId={null} onDescribe={() => {}} onChoose={() => {}} onClear={() => {}} />
      </MemoryRouter>
    </AppProviders>,
  );
}

const shownSource = async () => {
  const list = await screen.findByRole("list", { name: "Spell options" });
  await waitFor(() => expect(within(list).getAllByRole("listitem").length).toBeGreaterThan(0));
  return within(list).getAllByRole("listitem")[0]!.querySelector(".pick-src-name")!.textContent;
};

describe("the choice key and the shown copy", () => {
  it("keys a choice by type family, edition and normalized name", () => {
    expect(copyKey({ name: "  Fireball ", type: "spell" }, "dnd2024")).toBe(KEY_2024);
    expect(copyKey({ name: "Hill Dwarf", type: "subrace" }, "dnd2014")).toBe("subrace:dnd2014:hill dwarf");
    expect(copyKey({ name: "Dragonborn", type: "race" }, "dnd2024")).toBe("race:dnd2024:dragonborn");
  });

  it("the chosen copy, then a menu pick, then the copy always used, then the default order", () => {
    const vs = [FB_HOME, FB_SRD, FB_OLD];
    const always = { [KEY_2024]: "fb-srd" };
    expect(pickShown(vs, "dnd2024", null, null).id).toBe("fb-home");
    expect(pickShown(vs, "dnd2024", null, null, always).id).toBe("fb-srd");
    expect(pickShown(vs, "dnd2024", null, "fb-old", always).id).toBe("fb-old");
    expect(pickShown(vs, "dnd2024", "fb-home", "fb-old", always).id).toBe("fb-home");
  });

  it("a choice applies to its own edition only, and one naming a copy that is not offered is ignored", () => {
    const rows = buildRows([FIREBALL], null);
    expect(visibleRows(rows, NO_FILTER, "dnd2014", null, {}, { [KEY_2024]: "fb-srd" })[0]!.shown.id).toBe("fb-old");
    expect(visibleRows(rows, NO_FILTER, "dnd2024", null, {}, { [KEY_2024]: "not-installed" })[0]!.shown.id).toBe("fb-home");
  });

  it("the compendium sets a choice for the entry's own editions, both for an edition-neutral entry", () => {
    expect(copyEditions(["dnd2014"])).toEqual(["dnd2014"]);
    expect(copyEditions([])).toEqual(["dnd2024", "dnd2014"]);
  });
});

describe("the versions menu", () => {
  it("'Always use this copy' saves the shown copy in the app settings, and the picker then shows it by default", async () => {
    const api = apiWith([FIREBALL]);
    const save = vi.spyOn(api, "saveSettings");
    const view = renderPicker(api);
    expect(await shownSource()).toBe("Table Homebrew");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "3 versions of Fireball" }));
    await user.click(within(await screen.findByRole("listbox")).getByRole("option", { name: "SRD 5.2.1" }));
    await user.click(screen.getByRole("button", { name: "3 versions of Fireball" }));
    const always = screen.getByRole("checkbox", { name: "Always use this copy" }) as HTMLInputElement;
    expect(always.checked).toBe(false);
    await user.click(always);
    await waitFor(() => expect(save).toHaveBeenCalledWith({ patch: { preferredCopies: { [KEY_2024]: "fb-srd" } } }));
    expect((await api.getSettings()).preferredCopies).toEqual({ [KEY_2024]: "fb-srd" });
    await waitFor(() => expect(screen.getByRole("option", { name: "SRD 5.2.1, always used" })).toBeTruthy());
    view.unmount();

    // A new picker (no menu pick of its own) starts on the copy always used.
    renderPicker(api);
    expect(await shownSource()).toBe("SRD 5.2.1");
  });

  it("unticking forgets the choice", async () => {
    const api = apiWith([FIREBALL], { [KEY_2024]: "fb-srd", "feat:dnd2024:alert": "alert-srd" });
    renderPicker(api);
    expect(await shownSource()).toBe("SRD 5.2.1");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "3 versions of Fireball" }));
    await user.click(screen.getByRole("checkbox", { name: "Always use this copy" }));
    await waitFor(async () => expect((await api.getSettings()).preferredCopies).toEqual({ "feat:dnd2024:alert": "alert-srd" }));
    expect(await shownSource()).toBe("Table Homebrew");
  });

  it("a stored pick of another copy replays: the row keeps the chosen copy and nothing is switched", async () => {
    const api = apiWith([FIREBALL], { [KEY_2024]: "fb-srd" });
    const switchVersion = vi.spyOn(api, "switchVersion");
    renderPicker(api, { ...SLOT, chosen: { id: "fb-home", name: "Fireball", type: "spell", source: "table-homebrew" } } as SelectionSlot);
    expect(await shownSource()).toBe("Table Homebrew");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "3 versions of Fireball" }));
    const menu = await screen.findByRole("listbox");
    expect(within(menu).getByRole("option", { selected: true }).getAttribute("aria-label")).toBe("Table Homebrew, chosen");
    expect(within(menu).getByRole("option", { name: "SRD 5.2.1, always used" })).toBeTruthy();
    expect(switchVersion).not.toHaveBeenCalled();
  });
});

describe("the compendium entry", () => {
  const summary = (v: V): ElementSummary => ({ id: v.id, name: v.name, type: "spell", source: v.source!, sourceName: v.sourceName!, shortDescription: "", prerequisites: null, rulesets: v.rulesets! });
  // only the fields the entry reads
  const detail = (v: V) => ({ ...summary(v), descriptionHtml: "<p>A bright streak.</p>", facts: [], grants: [] }) as unknown as ElementDetail;

  function renderEntry(api: MockApi, id: string) {
    const all = [FB_HOME, FB_SRD, FB_OLD, ver("fb-ball", "SRD 5.2.1", ["dnd2024"], { name: "Delayed Blast Fireball" })];
    api.getElement = async ({ elementId }) => detail(all.find((v) => v.id === elementId)!);
    api.searchCompendium = async () => ({ total: all.length, items: all.map(summary) });
    render(
      <AppProviders api={api}>
        <MemoryRouter>
          <CompendiumBrowser initialSelected={id} onNavigate={() => {}} />
        </MemoryRouter>
      </AppProviders>,
    );
  }

  it("names the other copies of the same edition and saves 'Always use this copy' for the entry's edition", async () => {
    const api = apiWith([]);
    renderEntry(api, "fb-srd");
    const pane = await screen.findByRole("complementary", { name: "Element description" });
    expect(await within(pane).findByText("Also in: Table Homebrew")).toBeTruthy();
    await userEvent.setup().click(within(pane).getByRole("checkbox", { name: "Always use this copy" }));
    await waitFor(async () => expect((await api.getSettings()).preferredCopies).toEqual({ [KEY_2024]: "fb-srd" }));
    expect((within(pane).getByRole("checkbox", { name: "Always use this copy" }) as HTMLInputElement).checked).toBe(true);
  });

  it("an entry no other copy shares the name of offers no choice", async () => {
    const api = apiWith([]);
    renderEntry(api, "fb-ball");
    const pane = await screen.findByRole("complementary", { name: "Element description" });
    await within(pane).findByText("A bright streak.");
    await new Promise((r) => setTimeout(r, 20));
    expect(within(pane).queryByRole("checkbox", { name: "Always use this copy" })).toBeNull();
  });
});
