import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewForm } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { CreatorEnvValue } from "../pages/homebrew/env";
import { SpellListsField } from "../pages/homebrew/fields/SpellListsField";
import { ApiProvider } from "../state/ApiContext";
import type { Api } from "../api/contract";
import { renderApp } from "./utils";

// Homebrew WB-11 (spec 4.1, 13.4, 13.7): the spell form on the mock, which serves the real spell descriptor
// (`homebrew-kinds.ts`, generated from `borealis-authoring`): its sections and starting values, the spell lists control,
// and a saved spell offered by the spell picker with its level, school and components.

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };
const editor = () => screen.getByRole("region", { name: "Element editor" });
const classRow = (name: string) => ({ id: `ID_${name.toUpperCase()}`, name, type: "Class", summary: "", prerequisites: null, sourceName: "SRD", edition: null, draft: false });

describe("WB-11 spell form (mock)", () => {
  it("the mock's spell form is the backend's: required casting fields, sections, the spell lists control", () => {
    const spell = MOCK_KIND_DESCRIPTORS.find((k) => k.id === "spell")!;
    const byKey = new Map(spell.fields.map((f) => [f.key, f]));
    expect(["level", "school", "time", "duration", "range"].every((k) => byKey.get(k)?.required)).toBe(true);
    expect(byKey.has("tags")).toBe(false);
    expect(byKey.get("spellLists")).toMatchObject({ control: "spellLists", section: "Spell lists", bindings: ["supports", "data.classes"] });
    expect(byKey.get("level")?.choices?.[0]).toEqual({ value: "0", label: "Cantrip" });
    expect(byKey.get("verbal")).toMatchObject({ section: "Components", default: true });
  });

  it("a new spell shows its sections and starting values; the school is the one thing to choose; lists are check boxes", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    const update = vi.spyOn(api, "homebrewUpdateDraft");
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "New element" }));
    const flyout = await screen.findByRole("dialog", { name: "New element" });
    await user.type(within(flyout).getByLabelText("Name"), "Ember Lance");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));
    await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe("Ember Lance"));

    const titles = [...editor().querySelectorAll(".hb-section-title")].map((h) => h.textContent);
    expect(titles).toEqual(["Casting", "Components", "Spell lists", "Details"]);
    expect((screen.getByRole("combobox", { name: /^Level/ }) as HTMLSelectElement).value).toBe("1");
    expect((screen.getByRole("checkbox", { name: /^Verbal/ }) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByRole("checkbox", { name: /^Material \(M\)/ }) as HTMLInputElement).checked).toBe(false);
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \(1\)/ })).toBeTruthy());
    expect(within(screen.getByRole("list", { name: "Problems" })).getByRole("button", { name: "A spell needs a school." })).toBeTruthy();

    await user.selectOptions(screen.getByRole("combobox", { name: /^School/ }), "Evocation");
    const lists = screen.getByRole("group", { name: "Lists" });
    await user.click(await within(lists).findByRole("checkbox", { name: "Wizard" }));
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \(0\)/ })).toBeTruthy());
    const last = (): HomebrewForm => update.mock.lastCall![0].form;
    await waitFor(() => expect(last().fields).toMatchObject({ school: "Evocation", spellLists: ["Wizard"], level: "1" }));

    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Ember Lance" })).toBeTruthy();
    // the saved spell is offered by the spell picker with its Magic grid columns (spec 13.7)
    const rows = await api.homebrewReferenceOptions({ target: { types: ["Spell"] }, query: "ember lance", filters: {}, page: { offset: 0, limit: 10 } });
    expect(rows.items).toEqual([expect.objectContaining({ name: "Ember Lance", level: 1, school: "Evocation", spell: expect.objectContaining({ verbal: true, somatic: true, material: false, castingTime: "Action" }) })]);
  });
});

describe("SpellListsField", () => {
  /** The backend answers the declared lists (`Spell List` rows), or can't (then the loaded class names stand in). */
  const apiWith = (lists: string[] | null): Api => {
    const api = new MockApi();
    vi.spyOn(api, "homebrewReferenceOptions").mockImplementation(async () => {
      if (!lists) throw new Error("not implemented");
      return { total: lists.length, items: lists.map((n) => ({ ...classRow(n), id: n, type: "Spell List" })) };
    });
    return api;
  };
  const renderLists = (value: string[], onChange = vi.fn(), lists: string[] | null = null) => {
    render(
      <ApiProvider api={apiWith(lists)}>
        <CreatorEnvValue value={{ classes: [classRow("Wizard"), classRow("Cleric"), classRow("Wizard")] }}>
          <SpellListsField id="lists" label="Spell lists" value={value} onChange={onChange} />
        </CreatorEnvValue>
      </ApiProvider>,
    );
    return onChange;
  };

  it("offers the lists the backend says loaded classes declare", async () => {
    renderLists(["Druid"], vi.fn(), ["Bard", "Druid"]);
    const group = screen.getByRole("group", { name: "Spell lists" });
    await waitFor(() => expect(within(group).getAllByRole("checkbox").map((b) => b.getAttribute("aria-label"))).toEqual(["Bard", "Druid"]));
    expect((within(group).getByRole("checkbox", { name: "Druid" }) as HTMLInputElement).checked).toBe(true);
  });

  it("without the backend's lists: each loaded class once, then the spell's other lists, checked as the spell has them", async () => {
    renderLists(["Spell Saving Throw", "Cleric"]);
    const group = screen.getByRole("group", { name: "Spell lists" });
    await waitFor(() => expect(within(group).getAllByRole("checkbox")).toHaveLength(3));
    const boxes = within(group).getAllByRole("checkbox") as HTMLInputElement[];
    expect(boxes.map((b) => [b.getAttribute("aria-label"), b.checked])).toEqual([["Cleric", true], ["Wizard", false], ["Spell Saving Throw", true]]);
  });

  it("checking adds after the others, unchecking removes, a new list name is added once (any case)", async () => {
    const user = userEvent.setup();
    const onChange = renderLists(["Cleric"]);
    await user.click(await screen.findByRole("checkbox", { name: "Wizard" }));
    expect(onChange).toHaveBeenLastCalledWith(["Cleric", "Wizard"]);
    await user.click(screen.getByRole("checkbox", { name: "Cleric" }));
    expect(onChange).toHaveBeenLastCalledWith([]);
    await user.type(screen.getByRole("textbox", { name: "New spell list" }), "Mystic{Enter}");
    expect(onChange).toHaveBeenLastCalledWith(["Cleric", "Mystic"]);
    onChange.mockClear();
    await user.type(screen.getByRole("textbox", { name: "New spell list" }), "cleric");
    await user.click(screen.getByRole("button", { name: "Add spell list" }));
    expect(onChange).not.toHaveBeenCalled();
  });
});
