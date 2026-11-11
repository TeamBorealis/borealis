import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewFieldSpec, HomebrewForm, Ruleset } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { addChildIn } from "../api/mock/homebrew-class";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { groupFilter, groupOf } from "../pages/homebrew/blocks/OptionGroupPanel";
import { findForm } from "../pages/homebrew/class/children";
import { holdsValue, otherEdition } from "../pages/homebrew/editions";
import { renderApp } from "./utils";

// Homebrew P2a (1.5.0): children made inside children (a species' trait and the trait's lineage options, a background
// variant's own feature), a class option group made from the class's choice block, a field of the other edition shown
// while it holds a value, and the characters a saved child's removal would affect.

const editor = () => screen.getByRole("region", { name: "Element editor" });
const kindOf = (id: string) => MOCK_KIND_DESCRIPTORS.find((d) => d.id === id)!;
const fieldOf = (kind: string, key: string): HomebrewFieldSpec => kindOf(kind).fields.find((f) => f.key === key)!;

async function newElement(user: ReturnType<typeof userEvent.setup>, api: MockApi, edition: Ruleset, radio: RegExp, name: string) {
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition } });
  const update = vi.spyOn(api, "homebrewUpdateDraft");
  renderApp(`/homebrew/${pack.id}`, { api });
  await user.click(await screen.findByRole("button", { name: "New element" }));
  const flyout = await screen.findByRole("dialog", { name: "New element" });
  await user.click(within(flyout).getByRole("radio", { name: radio }));
  await user.type(within(flyout).getByLabelText("Name"), name);
  await user.click(within(flyout).getByRole("button", { name: "Create" }));
  await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe(name));
  return { pack, last: (): HomebrewForm => update.mock.lastCall![0].form };
}

describe("P2a children inside children, option groups, editions, removal (mock)", () => {
  it("the descriptors: species traits and a trait's lineage options are children; a variant has its own feature", () => {
    expect(fieldOf("species", "traits")).toMatchObject({ control: "children", picker: { types: ["Racial Trait"] } });
    expect(fieldOf("racialTrait", "lineage")).toMatchObject({ control: "children", editions: ["dnd2024"], picker: { types: ["Species Trait Option"] } });
    expect(fieldOf("backgroundVariant", "feature")).toMatchObject({ control: "children", picker: { types: ["Background Feature"] } });
    expect(kindOf("backgroundFeature").parentKinds).toEqual(["background", "backgroundVariant"]);
  });

  it("the helpers: a child added inside a child, a group's tag, a field's value of the other edition", () => {
    const leaf = (key: string, kind: string): HomebrewForm => ({ key, kind, fields: { name: key }, blocks: [], children: [], preserved: [] });
    const species: HomebrewForm = { ...leaf("e", "species"), fields: { traits: [{ child: "n1" }] }, children: [leaf("n1", "racialTrait")] };
    const next = addChildIn(species, "n1", leaf("n2", "speciesTraitOption"), MOCK_KIND_DESCRIPTORS)!;
    expect(findForm(next, "n1")?.fields.lineage).toEqual([{ child: "n2" }]);
    expect(findForm(next, "n2")?.kind).toBe("speciesTraitOption");
    expect(addChildIn(species, "zz", leaf("n2", "speciesTraitOption"), MOCK_KIND_DESCRIPTORS)).toBeNull();
    expect(groupOf(groupFilter("Ember Style"))).toBe("Ember Style");
    expect(groupOf(undefined)).toBeNull();
    const languages = fieldOf("background", "languages");
    expect(otherEdition(languages, "dnd2024")).toBe(true);
    expect(otherEdition(languages, "dnd2014")).toBe(false);
    expect(holdsValue(languages, [])).toBe(false);
    expect(holdsValue(languages, [{ id: "ID_LANGUAGE_DWARVISH", name: "Dwarvish", type: "Language" }])).toBe(true);
    expect(holdsValue(fieldOf("background", "personalityTraits"), { count: 2, items: [] })).toBe(false);
    expect(holdsValue(fieldOf("background", "personalityTraits"), { count: 2, items: ["I hum."] })).toBe(true);
    expect(holdsValue(fieldOf("background", "equipmentNote"), { type: "doc", content: [{ type: "paragraph" }] })).toBe(false);
  });

  it("a species: a trait made inside it, and the trait's lineage options made inside the trait", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const made = vi.spyOn(api, "homebrewNewDraft");
    const { last } = await newElement(user, api, "dnd2024", /^Species A 2024 species/, "Emberkin");
    const traits = within(editor()).getByRole("group", { name: "Traits" });
    await user.click(within(traits).getByRole("button", { name: "Add a racial trait" }));
    const traitDialog = await screen.findByRole("dialog", { name: /^Racial trait:/ });
    await user.type(within(traitDialog).getByRole("textbox", { name: /^Name/ }), "Ember Lineage");
    const options = within(traitDialog).getByRole("group", { name: "Lineage options" });
    await user.click(within(options).getByRole("button", { name: "Add a species trait option" }));
    expect(made).toHaveBeenLastCalledWith(expect.objectContaining({ kind: "speciesTraitOption", parentId: expect.any(String), parentKey: expect.any(String) }));
    const optionDialog = await screen.findByRole("dialog", { name: /^Species trait option:/ });
    await user.type(within(optionDialog).getByRole("textbox", { name: /^Name/ }), "Ash");
    await user.click(within(optionDialog).getByRole("button", { name: "Done" }));
    await waitFor(() => expect(within(options).getByRole("button", { name: "Ash" })).toBeTruthy());
    await waitFor(() => {
      const trait = last().children[0]!;
      expect(trait.fields.name).toBe("Ember Lineage");
      expect(trait.children.map((c) => c.fields.name)).toEqual(["Ash"]);
      expect(trait.fields.lineage).toEqual([{ child: trait.children[0]!.key }]);
    });
    await user.click(within(traitDialog).getByRole("button", { name: "Done" }));
    expect(within(traits).getByRole("button", { name: "Ember Lineage" })).toBeTruthy();
  });

  it("a 2014 background variant: its own feature made inside it", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const { last } = await newElement(user, api, "dnd2014", /^Background A background/, "Wanderer");
    const variants = within(editor()).getByRole("group", { name: "Variants" });
    await user.click(within(variants).getByRole("button", { name: "Add a background variant" }));
    const variantDialog = await screen.findByRole("dialog", { name: /^Background variant:/ });
    await user.type(within(variantDialog).getByRole("textbox", { name: /^Name/ }), "Night Walker");
    const own = within(variantDialog).getByRole("group", { name: "Its own feature" });
    await user.click(within(own).getByRole("button", { name: "Add a background feature" }));
    const featureDialog = await screen.findByRole("dialog", { name: /^Background feature:/ });
    await user.type(within(featureDialog).getByRole("textbox", { name: /^Name/ }), "Night Eyes");
    await user.click(within(featureDialog).getByRole("button", { name: "Done" }));
    await waitFor(() => {
      const variant = last().children[0]!;
      expect(variant.children.map((c) => c.fields.name)).toEqual(["Night Eyes"]);
      expect(variant.fields.feature).toEqual([{ child: variant.children[0]!.key }]);
      expect(last().fields.feature).toEqual([]);
    });
  });

  it("a 2024 background that holds a 2014 language shows it, with a note", async () => {
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
    const fresh = await api.homebrewNewDraft({ packId: pack.id, kind: "background" });
    const dwarvish = [{ id: "ID_LANGUAGE_DWARVISH", name: "Dwarvish", type: "Language" }];
    await api.homebrewUpdateDraft({ draftId: fresh.draftId, form: { ...fresh.form, fields: { ...fresh.form.fields, name: "Delver", languages: dwarvish } } });
    renderApp(`/homebrew/${pack.id}?draft=${fresh.draftId}`, { api });
    const field = await waitFor(() => {
      const f = editor().querySelector<HTMLElement>('[data-field="languages"]');
      expect(f).not.toBeNull();
      return f!;
    });
    expect(within(field).getByRole("note").textContent).toContain("belongs to the 2014 edition");
    expect(field.textContent).toContain("Dwarvish");
    // its other 2014 parts hold nothing and stay hidden
    expect(editor().querySelector('[data-field="feature"]')).toBeNull();
    expect(within(editor()).getAllByRole("tab").map((t) => t.textContent)).not.toContain("CHARACTERISTICS");
  });

  it("a class's choice of class options: the group and an option made in one flow", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const saved = vi.spyOn(api, "homebrewSaveDraft");
    const { pack, last } = await newElement(user, api, "dnd2024", /^Class A class/, "Ember Knight");
    await user.click(within(editor()).getByRole("tab", { name: "RULES" }));
    await user.click(screen.getByRole("button", { name: "Add rule" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Add a rule" })).getByRole("button", { name: "Lets the player choose ..." }));
    const dialog = await screen.findByRole("dialog", { name: "New rule: Lets the player choose ..." });
    await user.type(within(dialog).getByRole("combobox", { name: "What to choose" }), "Option");
    const panel = await within(dialog).findByRole("region", { name: "Class options of this choice" });
    await user.type(within(panel).getByRole("textbox", { name: "Group" }), "Ember Style");
    await user.type(within(panel).getByRole("textbox", { name: "New option" }), "Ash Guard");
    await user.click(within(panel).getByRole("button", { name: "Add option" }));
    await waitFor(() => expect(within(panel).getByRole("list", { name: "Options of Ember Style" }).textContent).toContain("Ash Guard"));
    expect(saved).toHaveBeenCalledTimes(1);
    await user.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => {
      const block = last().blocks.at(-1)!;
      expect(block.kind).toBe("select");
      expect(block.fields.type).toBe("Option");
      expect(block.fields.name).toBe("Ember Style");
      expect(groupOf(block.fields.supports)).toBe("Ember Style");
    });
    const listed = await api.homebrewListElements({ packId: pack.id });
    expect(listed.filter((e) => e.kind === "classOption").map((e) => e.name)).toEqual(["Ash Guard"]);
  });

  it("removing a feature that characters use asks first; Cancel keeps it", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const use = vi.spyOn(api, "homebrewChildUse").mockResolvedValue([{ characterId: "c1", name: "Ash Vale" }]);
    const { last } = await newElement(user, api, "dnd2024", /^Class A class/, "Ember Knight");
    await user.click(within(editor()).getByRole("tab", { name: "LEVELS" }));
    const grid = await screen.findByRole("table", { name: "Features by level" });
    const row = (n: number) => grid.querySelector<HTMLElement>(`tr[data-level="${n}"]`)!;
    await user.click(within(row(2)).getByRole("button", { name: "Add a feature at level 2" }));
    let dialog = await screen.findByRole("dialog", { name: /^Class feature:/ });
    await user.type(within(dialog).getByRole("textbox", { name: /^Name/ }), "Ember Strike");
    await user.click(within(dialog).getByRole("button", { name: "Remove from Ember Knight" }));
    const ask = await screen.findByRole("dialog", { name: "Remove" });
    expect(ask.textContent).toContain("1 character use “Ember Strike”: Ash Vale. They will lose it when you save.");
    expect(use).toHaveBeenCalledWith(expect.objectContaining({ childKey: expect.any(String) }));
    await user.click(within(ask).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Remove" })).toBeNull());
    expect(last().children.some((c) => c.fields.name === "Ember Strike")).toBe(true);

    dialog = screen.getByRole("dialog", { name: /^Class feature:/ });
    await user.click(within(dialog).getByRole("button", { name: "Remove from Ember Knight" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Remove" })).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(last().children.some((c) => c.fields.name === "Ember Strike")).toBe(false));
  });
});
