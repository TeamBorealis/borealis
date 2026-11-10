import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewForm } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { creatableKinds } from "../pages/homebrew/NewElementFlyout";
import { renderApp } from "./utils";

// Homebrew WB-15 (spec 4.4, 13.4): the species forms on the mock, which serves the real descriptors (`homebrew-kinds.ts`,
// generated from `borealis-authoring`): the 2024 Species and the 2014 Race, Subrace and Race variant with their size,
// speeds, senses, languages, ability score increases and traits; Racial Trait and Species Trait Option are made inside
// their parent.

const kindOf = (id: string) => MOCK_KIND_DESCRIPTORS.find((d) => d.id === id)!;
const editor = () => screen.getByRole("region", { name: "Element editor" });
const sections = () => [...editor().querySelectorAll(".hb-section-title")].map((h) => h.textContent);

async function newElement(user: ReturnType<typeof userEvent.setup>, api: MockApi, edition: "dnd2014" | "dnd2024", card: RegExp, name: string) {
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition } });
  const update = vi.spyOn(api, "homebrewUpdateDraft");
  renderApp(`/homebrew/${pack.id}`, { api });
  await user.click(await screen.findByRole("button", { name: "New element" }));
  const flyout = await screen.findByRole("dialog", { name: "New element" });
  await user.click(within(flyout).getByRole("radio", { name: card }));
  await user.type(within(flyout).getByLabelText("Name"), name);
  await user.click(within(flyout).getByRole("button", { name: "Create" }));
  await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe(name));
  return { last: (): HomebrewForm => update.mock.lastCall![0].form };
}

async function setNumber(user: ReturnType<typeof userEvent.setup>, label: string, value: number) {
  const box = within(editor()).getByRole("spinbutton", { name: label });
  await user.clear(box);
  await user.type(box, String(value));
}

describe("WB-15 species forms (mock)", () => {
  it("the mock's species forms are the backend's: rule-backed fields, 2014 parts, child kinds made inside their parent", () => {
    const species = kindOf("species");
    expect(species).toMatchObject({ group: "main", elementTypes: ["Species"], editions: ["dnd2024"] });
    const f = new Map(species.fields.map((x) => [x.key, x]));
    expect(f.get("size")).toMatchObject({ control: "chips", default: ["Medium"], section: "Species", bindings: ["rule:grant Size", "rule:select_size", "data.size"] });
    expect(f.get("speed")).toMatchObject({ control: "number", min: 0, max: 1000, default: 30, section: "Speed" });
    expect(f.get("darkvision")).toMatchObject({ control: "number", section: "Senses" });
    expect(f.get("traits")).toMatchObject({ control: "references", picker: { types: ["Racial Trait"] }, section: "Traits" });
    expect(f.has("strength")).toBe(false);
    const race = new Map(kindOf("race").fields.map((x) => [x.key, x]));
    expect(race.get("strength")).toMatchObject({ control: "number", min: -10, max: 10, section: "Ability score increases" });
    expect(race.get("languages")).toMatchObject({ control: "references", picker: { types: ["Language"] }, default: [{ id: "ID_LANGUAGE_COMMON", name: "Common", type: "Language" }] });
    expect(race.get("maleNames")).toMatchObject({ control: "chips", section: "Random names" });
    const sub = new Map(kindOf("subRace").fields.map((x) => [x.key, x]));
    expect(sub.get("parent")).toMatchObject({ control: "text", required: true, section: "Subrace" });
    expect(sub.has("size")).toBe(false);
    expect(kindOf("racialTrait")).toMatchObject({ group: "child", parentKinds: ["species", "race", "subRace", "raceVariant"] });
    expect(kindOf("speciesTraitOption")).toMatchObject({ group: "child", parentKinds: ["species", "racialTrait"] });
    const offered = creatableKinds(MOCK_KIND_DESCRIPTORS).map((d) => d.id);
    expect(offered).toEqual(expect.arrayContaining(["species", "race", "subRace", "raceVariant"]));
    expect(offered).not.toContain("racialTrait");
    expect(offered).not.toContain("speciesTraitOption");
  });

  it("a new species: its sections, a 35-foot speed and darkvision; saved, it is listed", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const { last } = await newElement(user, api, "dnd2024", /^Species/, "Emberkin");
    expect(sections()).toEqual(["Species", "Speed", "Senses", "Traits", "Details"]);
    expect(within(editor()).getByRole("button", { name: "Medium" }).getAttribute("aria-pressed")).toBe("true");
    expect((within(editor()).getByRole("spinbutton", { name: "Speed" }) as HTMLInputElement).value).toBe("30");

    await setNumber(user, "Speed", 35);
    await setNumber(user, "Darkvision", 120);
    await user.click(within(editor()).getByRole("button", { name: "Small" }));
    await waitFor(() => expect(last().fields).toMatchObject({ speed: 35, darkvision: 120, size: ["Medium", "Small"], creatureType: "Humanoid" }));
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \(0\)/ })).toBeTruthy());

    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Emberkin" })).toBeTruthy();
    const rows = await api.homebrewReferenceOptions({ target: { types: ["Species"] }, query: "emberkin", filters: {}, page: { offset: 0, limit: 10 } });
    expect(rows.items).toEqual([expect.objectContaining({ name: "Emberkin", type: "Species" })]);
  });

  it("a new subrace needs its parent race; a 2014 race picks its languages", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const { last } = await newElement(user, api, "dnd2014", /^Subrace/, "Ash Dwarf");
    expect(sections()).toEqual(["Subrace", "Ability score increases", "Speed", "Senses", "Languages", "Traits", "Details"]);
    await waitFor(() => expect(screen.getAllByText("A subrace needs a parent race.").length).toBeGreaterThan(0));
    await user.type(within(editor()).getByRole("textbox", { name: /^Parent race/ }), "Dwarf");
    await setNumber(user, "Charisma", 1);
    await waitFor(() => expect(last().fields).toMatchObject({ parent: "Dwarf", charisma: 1 }));
    await waitFor(() => expect(screen.queryAllByText("A subrace needs a parent race.")).toEqual([]));

    await user.click(within(editor()).getByRole("button", { name: "Pick: Languages" }));
    const picker = await screen.findByRole("dialog", { name: /Choose/ });
    await user.dblClick(await within(picker).findByText("Dwarvish"));
    await user.click(within(picker).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(last().fields.languages).toEqual([expect.objectContaining({ name: "Dwarvish", type: "language" })]));
    expect(within(editor()).getByRole("list", { name: "Languages" }).textContent).toContain("Dwarvish");
  });

  it("a new 2014 race starts with Common; a picked language joins it and Common keeps its name", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const { last } = await newElement(user, api, "dnd2014", /^Race A 2014/, "Stonefolk");
    expect(sections()).toEqual(["Race", "Ability score increases", "Speed", "Senses", "Languages", "Traits", "Random names", "Details"]);
    expect(within(editor()).getByRole("list", { name: "Languages" }).textContent).toContain("Common");
    await user.click(within(editor()).getByRole("button", { name: "Pick: Languages" }));
    const picker = await screen.findByRole("dialog", { name: /Choose/ });
    await user.dblClick(await within(picker).findByText("Giant"));
    await user.click(within(picker).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(last().fields.languages).toEqual([
      { id: "ID_LANGUAGE_COMMON", name: "Common", type: "Language" }, expect.objectContaining({ name: "Giant" }),
    ]));
  });
});
