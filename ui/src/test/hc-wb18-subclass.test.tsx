import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewChildEntry, HomebrewForm } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { refreshSubclass } from "../api/mock/homebrew-subclass";
import { REPEAT_KINDS } from "../pages/homebrew/blocks/BlockList";
import { creatableKinds } from "../pages/homebrew/NewElementFlyout";
import { renderApp } from "./utils";

// Homebrew WB-18 (spec 4.7, 4.8): the subclass, subclass feature and class option forms on the mock, which serves the
// real descriptors (`homebrew-kinds.ts`, generated from `borealis-authoring`): a subclass takes its class's subclass
// tag, level and subclass feature levels when the class is picked; its LEVELS grid marks those levels and makes its
// features inside it (`homebrew_new_draft {parentId}`).

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };
const editor = () => screen.getByRole("region", { name: "Element editor" });
const kind = (id: string) => MOCK_KIND_DESCRIPTORS.find((d) => d.id === id)!;

describe("WB-18 subclass and class option forms (mock)", () => {
  it("the mock's forms are the backend's: tabs, the class picker, the derived parts, the children", () => {
    const sub = kind("subclass");
    expect(sub.tabs.map((t) => t.label)).toEqual(["GENERAL", "LEVELS", "SPELLCASTING", "RULES", "DESCRIPTION"]);
    expect(sub.elementTypes).toEqual(["Archetype"]);
    const byKey = new Map(sub.fields.map((f) => [f.key, f]));
    expect(byKey.get("parentClass")).toMatchObject({ control: "reference", required: true, picker: { types: ["Class"] }, bindings: ["data.class", "data.class_name"] });
    expect(byKey.get("subclassTag")).toMatchObject({ readOnly: true, tab: "general" });
    expect(byKey.get("levelGained")).toMatchObject({ readOnly: true, bindings: ["data.level_gained"] });
    expect(byKey.get("subclassLevels")).toMatchObject({ readOnly: true, tab: "levels", control: "numbers" });
    expect(byKey.get("levels")).toMatchObject({ control: "levels", tab: "levels", picker: { types: ["Archetype Feature"] } });
    expect(byKey.has("allowDuplicate")).toBe(false);
    const feature = kind("subclassFeature");
    expect(feature).toMatchObject({ group: "child", parentKinds: ["subclass"], elementTypes: ["Archetype Feature"] });
    expect(feature.fields.filter((f) => f.tab === "spellcasting").map((f) => f.key)).toContain("casterType");
    expect(REPEAT_KINDS).toContain("subclassFeature");
    const option = kind("classOption");
    expect(option.elementTypes).toEqual(["Option"]);
    expect(option.fields.find((f) => f.key === "group")).toMatchObject({ control: "combo", required: true, allowCustom: true });
    expect(option.fields.find((f) => f.key === "className")).toMatchObject({ bindings: ["data.class_name"] });
    expect(option.fields.find((f) => f.key === "requirements")).toMatchObject({ control: "condition", section: "Option" });
    const creatable = creatableKinds(MOCK_KIND_DESCRIPTORS).map((d) => d.id);
    expect(creatable).toEqual(expect.arrayContaining(["subclass", "classOption"]));
    expect(creatable).not.toContain("subclassFeature");
  });

  it("refreshSubclass: the class picked gives the tag, the level and the subclass feature levels", () => {
    const form: HomebrewForm = { key: "e", kind: "subclass", fields: { name: "Ember Guard", parentClass: null }, blocks: [], children: [], preserved: [] };
    expect(refreshSubclass(form)).toBe(form);
    const picked = refreshSubclass({ ...form, fields: { ...form.fields, parentClass: { id: "fighter", name: "Fighter", type: "class" } } });
    expect(picked.fields).toMatchObject({ subclassTag: "Fighter Subclass", levelGained: 3, subclassLevels: [3, 7, 10, 15, 18] });
  });

  it("a new subclass of the Fighter: the class picked, a feature at its subclass level; saved", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    const update = vi.spyOn(api, "homebrewUpdateDraft");
    const child = vi.spyOn(api, "homebrewNewDraft");
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "New element" }));
    const flyout = await screen.findByRole("dialog", { name: "New element" });
    await user.click(within(flyout).getByRole("radio", { name: /^Subclass/ }));
    await user.type(within(flyout).getByLabelText("Name"), "Ember Guard");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));
    await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe("Ember Guard"));
    await waitFor(() => expect(screen.getAllByText(/^Choose the class this subclass belongs to/).length).toBeGreaterThan(0));

    await user.click(within(editor()).getByRole("button", { name: "Pick: Class" }));
    const picker = await screen.findByRole("dialog", { name: /Choose/ });
    await user.dblClick(await within(picker).findByText("Fighter"));
    await waitFor(() => expect(within(editor()).getByText("Fighter Subclass")).toBeTruthy());
    await waitFor(() => expect(screen.queryAllByText(/^Choose the class this subclass belongs to/)).toEqual([]));

    await user.click(within(editor()).getByRole("tab", { name: "LEVELS" }));
    expect(within(editor()).getByText("3, 7, 10, 15, 18")).toBeTruthy();
    const grid = await screen.findByRole("table", { name: "Features by level" });
    const row = (n: number) => grid.querySelector<HTMLElement>(`tr[data-level="${n}"]`)!;
    expect(row(3).className).toContain("hb-marked-level");
    expect(row(3).textContent).toContain("Subclass feature");
    expect(row(4).className).not.toContain("hb-marked-level");
    expect(within(editor()).queryByLabelText("New column")).toBeNull();

    await user.click(within(row(3)).getByRole("button", { name: "Add a feature at level 3" }));
    const dialog = await screen.findByRole("dialog", { name: /^Subclass feature:/ });
    expect(child).toHaveBeenLastCalledWith(expect.objectContaining({ kind: "subclassFeature", parentId: expect.any(String) }));
    await user.type(within(dialog).getByRole("textbox", { name: /^Name/ }), "Ember Ward");
    await user.click(within(dialog).getByRole("button", { name: "Done" }));
    await waitFor(() => expect(within(row(3)).getByRole("button", { name: "Ember Ward" })).toBeTruthy());
    expect(row(3).textContent).not.toContain("Subclass feature");

    const last = (): HomebrewForm => update.mock.lastCall![0].form;
    await waitFor(() => {
      const f = last();
      expect(f.fields.parentClass).toMatchObject({ id: "fighter", name: "Fighter" });
      expect(f.children.map((c) => [c.kind, c.fields.name])).toEqual([["subclassFeature", "Ember Ward"]]);
      expect((f.fields.levels as HomebrewChildEntry[])[0]).toEqual({ level: 3, child: f.children[0]!.key });
    });
    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Ember Guard" })).toBeTruthy();
  });
});
