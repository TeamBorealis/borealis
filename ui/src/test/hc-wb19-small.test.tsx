import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewForm } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { creatableKinds } from "../pages/homebrew/NewElementFlyout";
import { renderApp } from "./utils";

// Homebrew WB-19 (spec 4.10, 4.12): the small types and the generic "Other element" form on the mock, which serves the
// real descriptors (`homebrew-kinds.ts`, generated from `borealis-authoring`): a language's rarity, a deity's three
// required fields with their sentences, and the generic form's Type combo and property rows (names offered, attributes
// added) through the picker words.

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };
const editor = () => screen.getByRole("region", { name: "Element editor" });
const kind = (id: string) => MOCK_KIND_DESCRIPTORS.find((d) => d.id === id)!;
const SMALL = ["language", "proficiency", "condition", "damageType", "skill", "deity", "vision", "size", "alignment", "weaponProperty", "weaponGroup",
  "weaponCategory", "armorGroup", "masteryProperty", "focusGroup", "dragonmark", "rulesText"];

async function newElement(user: ReturnType<typeof userEvent.setup>, card: RegExp, name: string) {
  await user.click((await screen.findAllByRole("button", { name: /^New( element)?$/ }))[0]!);
  const flyout = await screen.findByRole("dialog", { name: "New element" });
  await user.click(within(flyout).getByRole("radio", { name: card }));
  await user.type(within(flyout).getByLabelText("Name"), name);
  await user.click(within(flyout).getByRole("button", { name: "Create" }));
  await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe(name));
}

describe("WB-19 small types and the generic form (mock)", () => {
  it("the mock's forms are the backend's: one compact form per small type, then the generic form", () => {
    const creatable = creatableKinds(MOCK_KIND_DESCRIPTORS).map((d) => d.id);
    expect(creatable.filter((id) => SMALL.includes(id))).toEqual(SMALL);
    expect(creatable[creatable.length - 1]).toBe("other");
    for (const id of SMALL) expect(kind(id).group, id).toBe("small");
    const field = (k: string, key: string) => kind(k).fields.find((f) => f.key === key);
    expect(field("language", "rarity")).toMatchObject({ control: "combo", choices: [{ value: "standard" }, { value: "exotic" }, { value: "secret" }] });
    expect(field("proficiency", "covers")).toMatchObject({ control: "references", picker: { types: ["Proficiency"], multiple: true }, bindings: ["rules:grant Proficiency"] });
    expect(field("condition", "listed")).toMatchObject({ control: "check", default: true, bindings: ["supports:Condition", "data.tag", "data.listed"] });
    expect(field("skill", "tags")).toMatchObject({ default: ["Skill"] });
    for (const key of ["alignment", "domains", "symbol"]) expect(field("deity", key)).toMatchObject({ required: true, help: expect.stringMatching(/^Every deity needs/) });
    expect(field("rulesText", "glossaryGroup")).toMatchObject({ control: "combo", allowCustom: true, bindings: ["data.tag", "supports:<group>"] });
    expect(field("other", "type")).toMatchObject({ control: "combo", allowCustom: true, required: true, picker: { types: ["Element Type"] } });
    expect(field("other", "setters")).toMatchObject({ control: "setters", picker: { types: ["Setter Name"] } });
    expect(field("other", "setters")?.expert).toBeUndefined();
  });

  it("a new language and a new deity: the rarity, the deity's sentences at its fields until filled; both saved", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    const update = vi.spyOn(api, "homebrewUpdateDraft");
    const last = (): HomebrewForm => update.mock.lastCall![0].form;
    renderApp(`/homebrew/${pack.id}`, { api });

    await newElement(user, /^Language A language/, "Old Tongue");
    await user.selectOptions(within(editor()).getByLabelText("Rarity"), "exotic");
    await user.type(within(editor()).getByRole("textbox", { name: /^Script/ }), "Dwarvish");
    await waitFor(() => expect(last().fields).toMatchObject({ rarity: "exotic", script: "Dwarvish" }));
    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Old Tongue" })).toBeTruthy();

    await newElement(user, /^Deity A god/, "Asha");
    for (const sentence of ["A deity needs an alignment.", "A deity needs domains.", "A deity needs a symbol."]) {
      await waitFor(() => expect(within(editor()).getAllByText(sentence).length).toBeGreaterThan(0));
    }
    await user.type(within(editor()).getByRole("combobox", { name: /^Alignment/ }), "Chaotic Good");
    await user.type(within(editor()).getByRole("combobox", { name: "New domains" }), "Light{Enter}");
    await user.type(within(editor()).getByRole("textbox", { name: /^Symbol/ }), "A flame in a hand");
    await waitFor(() => expect(within(editor()).queryAllByText(/^A deity needs/)).toEqual([]));
    await waitFor(() => expect(last().fields).toMatchObject({ alignment: "Chaotic Good", domains: ["Light"], symbol: "A flame in a hand" }));
    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Asha" })).toBeTruthy();
  });

  it("the generic form: its Type combo offers the open types, property names follow the type, attributes are added", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    const update = vi.spyOn(api, "homebrewUpdateDraft");
    const last = (): HomebrewForm => update.mock.lastCall![0].form;
    renderApp(`/homebrew/${pack.id}`, { api });

    await newElement(user, /^Other element/, "Ember Wolf");
    const type = within(editor()).getByRole("combobox", { name: /^Type/ });
    await user.click(within(editor()).getByRole("button", { name: "Show Type options" }));
    const list = await screen.findByRole("listbox");
    expect(within(list).getByRole("option", { name: "Familiar Trait" })).toBeTruthy();
    await user.click(within(list).getByRole("option", { name: "Companion" }));
    expect((type as HTMLInputElement).value).toBe("Companion");

    await user.click(within(editor()).getByRole("button", { name: "Add a property" }));
    await user.type(within(editor()).getByRole("combobox", { name: "Property 1: name" }), "bite");
    await user.type(within(editor()).getByRole("textbox", { name: "Property 1: value" }), "1d6");
    await user.type(within(editor()).getByRole("textbox", { name: "Property 1: new attribute name" }), "type");
    await user.type(within(editor()).getByRole("textbox", { name: "Property 1: new attribute value" }), "piercing");
    await user.click(within(editor()).getByRole("button", { name: "Add attribute" }));
    expect(within(editor()).getByText("type: piercing")).toBeTruthy();
    await waitFor(() => expect(last().fields).toMatchObject({ type: "Companion", setters: [{ name: "bite", value: "1d6", attributes: { type: "piercing" } }] }));
    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Ember Wolf" })).toBeTruthy();

    // a second companion is offered the first one's property names
    await newElement(user, /^Other element/, "Ember Hawk");
    await user.type(within(editor()).getByRole("combobox", { name: /^Type/ }), "Companion");
    await user.click(within(editor()).getByRole("button", { name: "Add a property" }));
    await user.click(within(editor()).getByRole("button", { name: "Show Property 1 name options" }));
    const names = await screen.findByRole("listbox");
    expect(within(names).getByRole("option", { name: "bite" })).toBeTruthy();
  });
});
