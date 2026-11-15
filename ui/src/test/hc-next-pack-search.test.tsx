import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ContentPack, HomebrewElementSummary, HomebrewKindDescriptor } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { searchElements } from "../pages/homebrew/search";
import { renderApp } from "./utils";

// H0eMM (spec 12.3): the HOMEBREW pack list's search box also searches the elements of every own pack, by the element
// list filter's rules (name, kind, tag, text), and a found element opens in its pack.

async function saved(api: MockApi, packId: string, kind: string, fields: Record<string, unknown>): Promise<string> {
  const v = await api.homebrewNewDraft({ packId, kind });
  const u = await api.homebrewUpdateDraft({ draftId: v.draftId, form: { ...v.form, fields: { ...v.form.fields, ...fields } } });
  return (await api.homebrewSaveDraft({ draftId: u.draftId })).elementIds[0]!;
}

const row = (over: Partial<HomebrewElementSummary>): HomebrewElementSummary => ({
  id: "ID_A", name: "Gloomspeak", kind: "language", elementType: "Language", parentId: null, edition: null,
  problems: { errors: 0, warnings: 0 }, hasDraft: false, tags: [], text: "", copiedText: false, ...over,
});
const KINDS = [{ id: "language", label: "Language", pluralLabel: "Languages" }, { id: "spell", label: "Spell", pluralLabel: "Spells" }] as HomebrewKindDescriptor[];
const pack = (id: string, name: string) => ({ id, name }) as ContentPack;

describe("H0eMM search every homebrew pack", () => {
  it("finds the elements of every pack where every word matches, sorted by name then pack; no words finds nothing", () => {
    // Arrange
    const lists = new Map([
      ["a", [row({ id: "A1", name: "Ember Lance", kind: "spell", elementType: "Spell" }), row({ id: "A2", name: "Gloomspeak", tags: ["Secret"] })]],
      ["b", [row({ id: "B1", name: "Ember Tongue", text: "Spoken by fire." })]],
    ]);
    const packs = [pack("a", "Ember Isles"), pack("b", "Ashlands")];

    // Act
    const ember = searchElements(packs, lists, "ember", KINDS);
    const fireTongue = searchElements(packs, lists, "fire language", KINDS);

    // Assert
    expect(ember.map((f) => [f.row.id, f.pack.name])).toEqual([["A1", "Ember Isles"], ["B1", "Ashlands"]]);
    expect(fireTongue.map((f) => f.row.id)).toEqual(["B1"]);
    expect(searchElements(packs, lists, "secret", KINDS).map((f) => f.row.id)).toEqual(["A2"]);
    expect(searchElements(packs, lists, "  ", KINDS)).toEqual([]);
  });

  it("the search box lists the elements of every pack; Enter on one opens it in its pack", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const isles = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
    const ash = await api.homebrewCreatePack({ request: { name: "Ashlands", code: "ASH", edition: "dnd2024" } });
    await saved(api, isles.id, "language", { name: "Gloomspeak" });
    await saved(api, ash.id, "language", { name: "Emberspeak" });
    await saved(api, isles.id, "language", { name: "Embertongue" });
    renderApp("/homebrew", { api });
    await user.type(await screen.findByRole("searchbox", { name: "Search packs and elements" }), "ember");

    const found = await screen.findByRole("grid", { name: "Elements in your packs" });
    await waitFor(() => expect(within(found).getAllByRole("row")).toHaveLength(3)); // head + 2
    expect(within(found).getByRole("row", { name: /Emberspeak.*Ashlands/ })).toBeTruthy();
    expect(within(found).getByRole("row", { name: /Embertongue.*Ember Isles/ })).toBeTruthy();
    expect(within(screen.getByRole("grid", { name: "Homebrew packs" })).getByText("Ember Isles")).toBeTruthy();

    await user.click(within(found).getByRole("row", { name: /Emberspeak/ }));
    expect(await screen.findByRole("button", { name: "Open element" })).toBeTruthy();
    found.focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect((screen.getByRole("textbox", { name: /^name$/i }) as HTMLInputElement).value).toBe("Emberspeak"));
  });
});
