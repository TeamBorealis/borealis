import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewChildEntry, HomebrewForm } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { addChild, newClassForm } from "../api/mock/homebrew-class";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { creatableKinds } from "../pages/homebrew/NewElementFlyout";
import { counterColumns, generatedOf, proficiencyBonus, withChildLevel, withoutChild } from "../pages/homebrew/class/children";
import { renderApp } from "./utils";

// Homebrew WB-16 (spec 4.5, 13.8): the class form on the mock, which serves the real class and Class Feature descriptors
// (`homebrew-kinds.ts`, generated from `borealis-authoring`): its sub-tabs, a new class's generated features, the LEVELS
// grid with a feature made inside the class (`homebrew_new_draft {parentId}`), the feature editor and the class table.

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };
const editor = () => screen.getByRole("region", { name: "Element editor" });
const levels = (f: HomebrewForm): HomebrewChildEntry[] => f.fields.levels as HomebrewChildEntry[];

describe("WB-16 class form (mock)", () => {
  it("the mock's class form is the backend's: the sub-tabs, the first-class-only parts, the children fields", () => {
    const k = MOCK_KIND_DESCRIPTORS.find((d) => d.id === "class")!;
    expect(k.tabs.map((t) => t.label)).toEqual(["GENERAL", "PROFICIENCIES", "LEVELS", "SPELLCASTING", "MULTICLASS", "EQUIPMENT", "RULES", "DESCRIPTION"]);
    const byKey = new Map(k.fields.map((f) => [f.key, f]));
    expect(byKey.get("hitDie")).toMatchObject({ control: "combo", required: true, bindings: ["setter:hd", "data.hit_die", "data.fixed_hp_per_level"] });
    expect(byKey.get("skills")).toMatchObject({ control: "skillChoice", tab: "proficiencies", section: "First class only" });
    expect(byKey.get("levels")).toMatchObject({ control: "levels", tab: "levels", picker: { types: ["Class Feature"] } });
    expect(byKey.get("table")).toMatchObject({ control: "classTable", bindings: ["data.progression"] });
    expect(byKey.get("equipment")).toMatchObject({ control: "package", tab: "equipment" });
    expect(byKey.get("multiclass")).toMatchObject({ control: "check", default: true });
    expect(k.fields.filter((f) => f.tab === "spellcasting")).toEqual([]);
    const feature = MOCK_KIND_DESCRIPTORS.find((d) => d.id === "classFeature")!;
    expect(feature).toMatchObject({ group: "child", parentKinds: ["class"], elementTypes: ["Class Feature"] });
    expect(feature.fields.find((f) => f.key === "epicBoon")).toMatchObject({ editions: ["dnd2024"] });
    expect(creatableKinds(MOCK_KIND_DESCRIPTORS).map((d) => d.id)).toContain("class");
    expect(creatableKinds(MOCK_KIND_DESCRIPTORS).map((d) => d.id)).not.toContain("classFeature");
  });

  it("a new class's generated features, and the children helpers", () => {
    const c2024 = newClassForm(MOCK_KIND_DESCRIPTORS, "dnd2024");
    expect(levels(c2024)).toEqual([{ level: 3, child: "n2" }, { level: 4, child: "n1" }, { level: 19, child: "n3" }]);
    const gen = generatedOf(c2024);
    expect(gen.asi?.fields.asiLevels).toEqual([4, 8, 12, 16]);
    expect(gen.subclass?.fields.subclassLabel).toBe("Subclass");
    expect(gen.epicBoon?.fields.epicBoon).toBe(true);
    const c2014 = newClassForm(MOCK_KIND_DESCRIPTORS, "dnd2014");
    expect(generatedOf(c2014).asi?.fields.asiLevels).toEqual([4, 8, 12, 16, 19]);
    expect(generatedOf(c2014).epicBoon).toBeUndefined();
    const feature = MOCK_KIND_DESCRIPTORS.find((d) => d.id === "classFeature")!;
    const added = addChild(c2024, { key: "n9", kind: feature.id, fields: { name: "Vow" }, blocks: [], children: [], preserved: [] }, MOCK_KIND_DESCRIPTORS)!;
    expect(levels(added).at(-1)).toEqual({ level: 1, child: "n9" });
    expect(levels(withChildLevel(added, "levels", "n9", 6)).at(-1)).toEqual({ level: 6, child: "n9" });
    const removed = withoutChild(added, "n9", ["levels"]);
    expect(removed.children.map((c) => c.key)).toEqual(["n1", "n2", "n3"]);
    expect(levels(removed).some((e) => e.child === "n9")).toBe(false);
    expect(proficiencyBonus(1)).toBe(2);
    expect(proficiencyBonus(17)).toBe(6);
    expect(counterColumns([{ key: "proficiency_bonus", label: "", values: [] }, { key: "rage", label: "Rage", values: [] }]).map((c) => c.index)).toEqual([1]);
  });

  it("a new class: the LEVELS grid, a feature added at level 2 and edited, a counter column; saved", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    const update = vi.spyOn(api, "homebrewUpdateDraft");
    const child = vi.spyOn(api, "homebrewNewDraft");
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "New element" }));
    const flyout = await screen.findByRole("dialog", { name: "New element" });
    await user.click(within(flyout).getByRole("radio", { name: /^Class/ }));
    await user.type(within(flyout).getByLabelText("Name"), "Gloom Knight");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));
    await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe("Gloom Knight"));

    await user.click(within(editor()).getByRole("tab", { name: "LEVELS" }));
    const grid = await screen.findByRole("table", { name: "Features by level" });
    const row = (n: number) => grid.querySelector<HTMLElement>(`tr[data-level="${n}"]`)!;
    expect(within(row(4)).getByRole("button", { name: "Ability Score Improvement" })).toBeTruthy();
    expect(within(row(3)).getByRole("button", { name: "Subclass" })).toBeTruthy();
    expect(within(row(19)).getByRole("button", { name: "Epic Boon" })).toBeTruthy();
    expect(row(5).textContent).toContain("+3");

    // a feature at level 2, made inside the class by the backend
    await user.click(within(row(2)).getByRole("button", { name: "Add a feature at level 2" }));
    const dialog = await screen.findByRole("dialog", { name: /^Class feature:/ });
    expect(child).toHaveBeenLastCalledWith(expect.objectContaining({ kind: "classFeature", parentId: expect.any(String) }));
    await user.type(within(dialog).getByRole("textbox", { name: /^Name/ }), "Shadow Step");
    expect(within(dialog).getByRole("spinbutton", { name: "Level gained" })).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Done" }));
    await waitFor(() => expect(within(row(2)).getByRole("button", { name: "Shadow Step" })).toBeTruthy());

    // ability score improvements: 6 added
    const asi = screen.getByRole("group", { name: "Ability score improvements at levels" });
    await user.click(within(asi).getByRole("button", { name: "6" }));

    // a counter column
    await user.type(screen.getByLabelText("New column"), "Vows");
    await user.click(screen.getByRole("button", { name: "Add column" }));
    const cell = await screen.findByRole("textbox", { name: "Vows at level 3" });
    await user.clear(cell);
    await user.type(cell, "2");

    const last = (): HomebrewForm => update.mock.lastCall![0].form;
    await waitFor(() => {
      const f = last();
      expect(f.children.map((c) => c.fields.name)).toEqual(["Ability Score Improvement", "Subclass", "Epic Boon", "Shadow Step"]);
      expect(levels(f).find((e) => e.child === f.children[3]!.key)?.level).toBe(2);
      expect(f.children[0]!.fields.asiLevels).toEqual([4, 6, 8, 12, 16]);
      expect((f.fields.table as { label: string; values: unknown[] }[])[0]).toMatchObject({ label: "Vows" });
      expect((f.fields.table as { values: unknown[] }[])[0]!.values[2]).toBe(2);
    });

    await user.click(within(editor()).getByRole("tab", { name: "GENERAL" }));
    await user.selectOptions(screen.getByRole("combobox", { name: /^Hit die/ }), "d10");
    await waitFor(() => expect(last().fields.hitDie).toBe("d10"));
    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Gloom Knight" })).toBeTruthy();
  });
});
