import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewForm } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { spellcastingChild } from "../pages/homebrew/caster/SpellcastingTab";
import { gridOf, isSpellcastingColumn, levelsOf, slotGrid, topSlotLevel, withCell, withLevels } from "../pages/homebrew/caster/slots";
import { tabOf } from "../pages/homebrew/problems";
import { renderApp } from "./utils";

// Homebrew WB-17 (spec 4.6): the spellcasting section on the mock, which serves the real Class Feature and feat
// descriptors (`homebrew-kinds.ts`, generated from `borealis-authoring`): the slot tables the SPELLCASTING tab shows
// per caster type (the backend's `caster::section` tables), and the class's SPELLCASTING tab adding a Spellcasting
// feature, setting a caster type, a custom table and the counts per level.

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };
const editor = () => screen.getByRole("region", { name: "Element editor" });

describe("WB-17 spellcasting section", () => {
  it("each caster type's slots are the SRD tables", () => {
    expect(slotGrid("full")[0]).toEqual([2, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect(slotGrid("full")[19]).toEqual([4, 3, 3, 3, 3, 2, 2, 1, 1]);
    expect(slotGrid("halfUp")[0]).toEqual([2, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect(slotGrid("halfUp")[4]).toEqual([4, 2, 0, 0, 0, 0, 0, 0, 0]);
    expect(slotGrid("halfDown")[0]).toEqual([0, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect(slotGrid("halfDown")[1]).toEqual([2, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect(slotGrid("third")[2]).toEqual([2, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect(slotGrid("third")[19]).toEqual([4, 3, 3, 1, 0, 0, 0, 0, 0]);
    expect(slotGrid("pact")[10]).toEqual([0, 0, 0, 0, 3, 0, 0, 0, 0]);
    expect(topSlotLevel(slotGrid("halfUp"))).toBe(5);
    expect(topSlotLevel(slotGrid(""))).toBe(0);
    const custom = withCell(slotGrid(""), 3, 1, 2);
    expect(gridOf(custom)).toEqual(custom);
    expect(slotGrid("custom", custom)[2]).toEqual([2, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect(gridOf([[1]])).toBeNull();
    expect(isSpellcastingColumn("slot_3")).toBe(true);
    expect(isSpellcastingColumn("pact_slot_level")).toBe(true);
    expect(isSpellcastingColumn("rage_damage")).toBe(false);
    expect(levelsOf({ levels: [1, 2], statistic: true })[1]).toBe(2);
    expect(withLevels({ levels: [], statistic: true }, [3])).toEqual({ levels: [3], statistic: true });
    expect(withLevels([], [3])).toEqual([3]);
  });

  it("the mock's section is the backend's: a SPELLCASTING tab on the Class Feature and the feat", () => {
    for (const id of ["classFeature", "feat"]) {
      const k = MOCK_KIND_DESCRIPTORS.find((d) => d.id === id)!;
      expect(k.tabs.map((t) => t.label)).toContain("SPELLCASTING");
      const section = k.fields.filter((f) => f.tab === "spellcasting");
      expect(section.map((f) => f.key)).toEqual([
        "casts", "casterName", "ability", "preparation", "spellList", "swap", "casterType", "slotTable", "multiclassSlots",
        "prepared", "prepareFormula", "cantrips", "known", "preparedChange",
      ]);
      const byKey = new Map(section.map((f) => [f.key, f]));
      expect(byKey.get("casterType")?.choices?.map((c) => c.value)).toEqual(["", "full", "halfUp", "halfDown", "third", "pact", "custom"]);
      expect(byKey.get("slotTable")).toMatchObject({ control: "slotTable" });
      expect(byKey.get("cantrips")).toMatchObject({ control: "levelCounts" });
      expect(byKey.get("prepareFormula")?.editions).toEqual(["dnd2014"]);
      expect(byKey.get("casts")?.default).toBe(false);
    }
    const kind = MOCK_KIND_DESCRIPTORS.find((d) => d.id === "class")!;
    expect(tabOf({ severity: "error", message: "x", path: "children.3.fields.ability", elementId: null, code: "form" }, kind)).toBe("spellcasting");
    expect(tabOf({ severity: "error", message: "x", path: "children.3.fields.name", elementId: null, code: "form" }, kind)).toBe("levels");
    const form = { key: "e", kind: "class", fields: {}, blocks: [], preserved: [], children: [
      { key: "n1", kind: "classFeature", fields: { name: "Pact Magic" }, blocks: [], preserved: [], children: [] },
    ] } as HomebrewForm;
    expect(spellcastingChild(form)).toBe(0);
  });

  it("a class's SPELLCASTING tab: add spellcasting, a caster type's slot table, a custom table, counts; saved", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    const update = vi.spyOn(api, "homebrewUpdateDraft");
    const child = vi.spyOn(api, "homebrewNewDraft");
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "New element" }));
    const flyout = await screen.findByRole("dialog", { name: "New element" });
    await user.click(within(flyout).getByRole("radio", { name: /^Class A class/ }));
    await user.type(within(flyout).getByLabelText("Name"), "Warden");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));
    await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe("Warden"));

    await user.click(within(editor()).getByRole("tab", { name: "SPELLCASTING" }));
    expect(screen.getByText("This class doesn't cast spells.")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Add spellcasting" }));
    await waitFor(() => expect(child).toHaveBeenLastCalledWith(expect.objectContaining({ kind: "classFeature", parentId: expect.any(String) })));
    const table = await screen.findByRole("table", { name: "Spell slots and spells by level" });
    const row = (n: number) => table.querySelector<HTMLElement>(`tr[data-level="${n}"]`)!;
    const slots = (n: number) => Array.from(row(n).querySelectorAll(".hb-slot-cell")).map((c) => c.textContent);
    expect(slots(1)).toEqual(["2", "-", "-", "-", "-", "-", "-", "-", "-"]);

    await user.selectOptions(screen.getByRole("combobox", { name: /^Caster type/ }), "halfUp");
    await waitFor(() => expect(slots(5)).toEqual(["4", "2", "-", "-", "-"]));
    await user.selectOptions(screen.getByRole("combobox", { name: /^Spellcasting ability/ }), "Wisdom");
    await user.type(screen.getByRole("textbox", { name: /^Spell list/ }), "Ranger");
    const prepared = within(row(5)).getByRole("textbox", { name: "Prepared at level 5" });
    await user.clear(prepared);
    await user.type(prepared, "6");

    // a custom table starts from the half caster's slots and is edited cell by cell
    await user.selectOptions(screen.getByRole("combobox", { name: /^Caster type/ }), "custom");
    const cell = await screen.findByRole("textbox", { name: "3rd-level slots at level 20" });
    await user.clear(cell);
    await user.type(cell, "1");

    const sc = (): HomebrewForm => {
      const f: HomebrewForm = update.mock.lastCall![0].form;
      return f.children.find((c) => c.fields.casts === true)!;
    };
    await waitFor(() => {
      const f = sc();
      expect(f.fields).toMatchObject({ name: "Spellcasting", casterType: "custom", ability: "Wisdom", spellList: "Ranger", preparation: "prepared" });
      const grid = f.fields.slotTable as number[][];
      expect(grid[4]).toEqual([4, 2, 0, 0, 0, 0, 0, 0, 0]);
      expect(grid[19]![2]).toBe(1);
      expect((f.fields.prepared as number[])[4]).toBe(6);
    });
    await user.click(within(editor()).getByRole("tab", { name: "LEVELS" }));
    const grid = await screen.findByRole("table", { name: "Features by level" });
    expect(within(grid.querySelector<HTMLElement>('tr[data-level="1"]')!).getByRole("button", { name: "Spellcasting" })).toBeTruthy();
    await user.click(within(editor()).getByRole("tab", { name: "GENERAL" }));
    await user.selectOptions(screen.getByRole("combobox", { name: /^Hit die/ }), "d10");
    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Warden" })).toBeTruthy();
  });
});
