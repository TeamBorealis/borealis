import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewForm } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { creatableKinds } from "../pages/homebrew/NewElementFlyout";
import { renderApp } from "./utils";

// Homebrew WB-14 (spec 4.3, 13.4): the background form on the mock, which serves the real descriptors (generated from
// `borealis-authoring`): one compact form per edition (the other edition's fields and tabs hidden), the origin feat
// with the feat's own choices, the 2024 sentences at their fields, the 2014 characteristics and choices.

const kindOf = (id: string) => MOCK_KIND_DESCRIPTORS.find((d) => d.id === id)!;
const editor = () => screen.getByRole("region", { name: "Element editor" });
const sections = () => [...editor().querySelectorAll(".hb-section-title")].map((h) => h.textContent);
const tabs = () => within(editor()).getAllByRole("tab").map((t) => t.textContent);

async function newBackground(user: ReturnType<typeof userEvent.setup>, api: MockApi, edition: "dnd2014" | "dnd2024", name: string) {
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition } });
  const update = vi.spyOn(api, "homebrewUpdateDraft");
  renderApp(`/homebrew/${pack.id}`, { api });
  await user.click(await screen.findByRole("button", { name: "New element" }));
  const flyout = await screen.findByRole("dialog", { name: "New element" });
  await user.click(within(flyout).getByRole("radio", { name: /^Background A background/ }));
  await user.type(within(flyout).getByLabelText("Name"), name);
  await user.click(within(flyout).getByRole("button", { name: "Create" }));
  await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe(name));
  return { last: (): HomebrewForm => update.mock.lastCall![0].form };
}

async function pick(user: ReturnType<typeof userEvent.setup>, button: string, names: readonly string[]) {
  await user.click(within(editor()).getByRole("button", { name: button }));
  const picker = await screen.findByRole("dialog", { name: /Choose/ });
  for (const n of names) await user.dblClick(await within(picker).findByText(n));
  const ok = within(picker).queryByRole("button", { name: "OK" });
  if (ok) await user.click(ok);
  await waitFor(() => expect(screen.queryByRole("dialog", { name: /Choose/ })).toBeNull());
}

describe("WB-14 background form (mock)", () => {
  it("the mock's background forms are the backend's: per-edition fields, its children made inside it", () => {
    const bg = kindOf("background");
    expect(bg).toMatchObject({ group: "main", elementTypes: ["Background"], editions: ["dnd2014", "dnd2024"] });
    const f = new Map(bg.fields.map((x) => [x.key, x]));
    expect(f.get("abilityScores")).toMatchObject({ control: "chips", editions: ["dnd2024"], section: "Ability scores" });
    expect(f.get("originFeat")).toMatchObject({ control: "featGrant", picker: { types: ["Feat"] }, editions: ["dnd2024"] });
    expect(f.get("skills")).toMatchObject({ control: "references", picker: { types: ["Proficiency"], tags: ["Skill"], multiple: true } });
    expect(f.get("choices")).toMatchObject({ control: "choices", section: "Proficiencies" });
    expect(f.get("feature")).toMatchObject({ control: "children", editions: ["dnd2014"] });
    expect(f.get("personalityTraits")).toMatchObject({ control: "writtenList", tab: "characteristics", default: { count: 2, items: [] } });
    expect(f.get("equipment")).toMatchObject({ control: "package", tab: "equipment" });
    expect(f.has("allowDuplicate")).toBe(false);
    for (const k of ["backgroundFeature", "backgroundVariant", "backgroundCharacteristics"]) expect(kindOf(k)).toMatchObject({ group: "child", parentKinds: ["background"] });
    expect(kindOf("equipmentPackage").parentKinds).toContain("background");
    const offered = creatableKinds(MOCK_KIND_DESCRIPTORS).map((d) => d.id);
    expect(offered).toContain("background");
    expect(offered).not.toContain("backgroundFeature");
  });

  it("a new 2024 background: three abilities, an origin feat with its spell list and two skills, then saved", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const { last } = await newBackground(user, api, "dnd2024", "Ember Warden");
    expect(sections()).toEqual(["Ability scores", "Origin feat", "Proficiencies", "Details"]);
    expect(tabs()).toEqual(["GENERAL", "EQUIPMENT", "RULES", "DESCRIPTION", "DATA"]);
    for (const s of ["Pick three abilities.", "Choose the origin feat.", "Pick two skills."]) {
      await waitFor(() => expect(screen.getAllByText(s).length).toBeGreaterThan(0));
    }
    for (const a of ["Strength", "Constitution", "Wisdom"]) await user.click(within(editor()).getByRole("button", { name: a }));
    await waitFor(() => expect(last().fields.abilityScores).toEqual(["Strength", "Constitution", "Wisdom"]));

    await pick(user, "Pick: Origin feat", ["Magic Initiate"]);
    const list = await within(editor()).findByRole("combobox", { name: "Spell List of the feat" });
    expect(within(editor()).getByRole("combobox", { name: "Spellcasting Ability of the feat" })).toBeTruthy();
    await user.selectOptions(list, "Druid");
    await waitFor(() => expect(last().fields.originFeat).toMatchObject({ feat: { name: "Magic Initiate" }, params: { spell_list: "Druid" } }));

    await pick(user, "Pick: Skill proficiencies", ["Insight", "Religion"]);
    await waitFor(() => expect(last().fields.skills).toEqual([expect.objectContaining({ name: "Insight" }), expect.objectContaining({ name: "Religion" })]));
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \(0\)/ })).toBeTruthy());

    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Ember Warden" })).toBeTruthy();
    const rows = await api.homebrewReferenceOptions({ target: { types: ["Background"] }, query: "ember", filters: {}, page: { offset: 0, limit: 10 } });
    expect(rows.items).toEqual([expect.objectContaining({ name: "Ember Warden", type: "Background" })]);
  });

  it("a new 2014 background: its characteristics, numbered, and a language choice", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const { last } = await newBackground(user, api, "dnd2014", "Hermit");
    expect(sections()).toEqual(["Proficiencies", "Feature", "Details"]);
    expect(tabs()).toEqual(["GENERAL", "CHARACTERISTICS", "EQUIPMENT", "RULES", "DESCRIPTION", "DATA"]);
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \(0\)/ })).toBeTruthy());

    await user.click(within(editor()).getByRole("button", { name: "Add a choice" }));
    await waitFor(() => expect(last().fields.choices).toEqual([{ kind: "language", count: 1, from: null, name: "" }]));
    await user.selectOptions(within(editor()).getByRole("combobox", { name: "Choice 1: what" }), "proficiency");
    await waitFor(() => expect(last().fields.choices).toEqual([expect.objectContaining({ kind: "proficiency" })]));

    await user.click(within(editor()).getByRole("tab", { name: "CHARACTERISTICS" }));
    const traits = within(editor()).getByRole("group", { name: "Personality traits" });
    expect((within(traits).getByRole("spinbutton", { name: "Personality traits: how many" }) as HTMLInputElement).value).toBe("2");
    await user.click(within(traits).getByRole("button", { name: "Add an entry" }));
    await user.click(within(traits).getByRole("button", { name: "Add an entry" }));
    await user.type(within(traits).getByRole("textbox", { name: "Personality traits 1" }), "I am quiet.");
    await user.type(within(traits).getByRole("textbox", { name: "Personality traits 2" }), "I hum.");
    await waitFor(() => expect(last().fields.personalityTraits).toEqual({ count: 2, items: ["I am quiet.", "I hum."] }));
    expect(within(traits).getAllByRole("listitem").map((li) => li.querySelector(".hb-list-number")?.textContent)).toEqual(["1", "2"]);
  });
});
