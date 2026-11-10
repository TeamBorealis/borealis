import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewForm } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { featPrerequisite } from "../api/mock/homebrew-feat";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { creatableKinds } from "../pages/homebrew/NewElementFlyout";
import { renderApp } from "./utils";

// Homebrew WB-13 (spec 4.2, 13.4): the feat form on the mock, which serves the real feat and Feat Feature descriptors
// (`homebrew-kinds.ts`, generated from `borealis-authoring`): its sections, the 2024 category, one prerequisite condition,
// taking it again, and a saved feat offered by a feat picker with its prerequisite sentence.

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };
const editor = () => screen.getByRole("region", { name: "Element editor" });
const feat = (fields: Record<string, unknown>): HomebrewForm => ({ kind: "feat", fields, blocks: [], children: [], preserved: [] });
const model = (items: unknown[]) => ({ value: { shape: "model", model: { node: "group", mode: "all", items } } });
const row = (r: Record<string, unknown>) => ({ node: "row", row: r });

describe("WB-13 feat form (mock)", () => {
  it("the mock's feat form is the backend's: the category, one condition for every prerequisite location, taking it again", () => {
    const k = MOCK_KIND_DESCRIPTORS.find((d) => d.id === "feat")!;
    const byKey = new Map(k.fields.map((f) => [f.key, f]));
    expect(byKey.get("category")).toMatchObject({ control: "combo", allowCustom: true, section: "Feat", bindings: ["setter:category", "supports:<Category> Feat|Feat", "data.category"] });
    expect(byKey.get("category")?.choices?.map((c) => c.value)).toEqual(["Origin", "General", "Fighting Style", "Epic Boon"]);
    expect(byKey.get("requirements")).toMatchObject({ control: "condition", section: "Prerequisite", bindings: ["requirements", "data.prerequisite"] });
    expect(byKey.get("prerequisiteText")).toMatchObject({ bindings: ["prerequisite"], default: "" });
    expect(byKey.get("repeatable")).toMatchObject({ control: "check", default: false, bindings: ["setter:allow duplicate", "data.repeatable.allowed"] });
    expect(byKey.has("allowDuplicate")).toBe(false);
    const child = MOCK_KIND_DESCRIPTORS.find((d) => d.id === "featFeature")!;
    expect(child).toMatchObject({ group: "child", parentKinds: ["feat"], elementTypes: ["Feat Feature"] });
    // a Feat Feature is made inside its feat, not from New element
    expect(creatableKinds(MOCK_KIND_DESCRIPTORS).map((d) => d.id)).not.toContain("featFeature");
  });

  it("the prerequisite as shown: the typed sentence, else the one written from the condition", () => {
    const grappler = model([row({ row: "characterLevel", min: 4 }), { node: "group", mode: "any", items: [row({ row: "ability", ability: "strength", min: 13 }), row({ row: "ability", ability: "dexterity", min: 13 })] }]);
    expect(featPrerequisite(feat({ requirements: grappler, prerequisiteText: "" }))).toBe("Level 4+, Strength or Dexterity 13+");
    expect(featPrerequisite(feat({ requirements: model([row({ row: "feature", name: "Fighting Style" })]) }))).toBe("Fighting Style Feature");
    expect(featPrerequisite(feat({ requirements: grappler, prerequisiteText: " A strong grip " }))).toBe("A strong grip");
    expect(featPrerequisite(feat({ requirements: model([]) }))).toBeNull();
    expect(featPrerequisite({ ...feat({ prerequisiteText: "x" }), kind: "spell" })).toBeNull();
  });

  it("a new feat: sections, a General category, a level prerequisite; saved, a feat picker offers it with its prerequisite", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    const update = vi.spyOn(api, "homebrewUpdateDraft");
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "New element" }));
    const flyout = await screen.findByRole("dialog", { name: "New element" });
    await user.click(within(flyout).getByRole("radio", { name: /^Feat/ }));
    await user.type(within(flyout).getByLabelText("Name"), "Ember Heart");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));
    await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe("Ember Heart"));

    const titles = [...editor().querySelectorAll(".hb-section-title")].map((h) => h.textContent);
    expect(titles).toEqual(["Feat", "Prerequisite", "Taking it again", "Details"]);
    expect((screen.getByRole("checkbox", { name: /^Can be taken more than once/ }) as HTMLInputElement).checked).toBe(false);

    await user.click(screen.getByRole("button", { name: "Show Category options" }));
    await user.click(await screen.findByRole("option", { name: "General" }));
    await user.click(within(editor()).getByRole("button", { name: "Add condition" }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Prerequisite, part 1: test" }), "characterLevel");
    const last = (): HomebrewForm => update.mock.lastCall![0].form;
    await waitFor(() => expect(last().fields).toMatchObject({ category: "General", requirements: model([row({ row: "characterLevel", min: 4 })]) }));
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \(0\)/ })).toBeTruthy());
    const pane = screen.getAllByRole("complementary", { name: "Description" }).at(-1)!;
    await waitFor(() => expect(pane.textContent).toContain("Level 4+"));

    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Ember Heart" })).toBeTruthy();
    const rows = await api.homebrewReferenceOptions({ target: { types: ["Feat"] }, query: "ember heart", filters: {}, page: { offset: 0, limit: 10 } });
    expect(rows.items).toEqual([expect.objectContaining({ name: "Ember Heart", type: "Feat", prerequisites: "Level 4+" })]);
  });
});
