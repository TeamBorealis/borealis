import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ItemSummary } from "../api/contract";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

/** A generic magic armor is added on a base picked in a second list (specs/ui-inventory.md 6.1, ui-workflows W4.3). */
const summary = (id: string, name: string, category: string, weight: number): ItemSummary => ({
  id, name, type: "item", source: "sample", sourceName: "Sample", shortDescription: "", prerequisites: null,
  rulesets: ["dnd2014"], category, costCp: 0, weight, requiresAttunement: false,
});
const PLUS_ONE = summary("plus-one-armor", "Armor, +1", "Magic Armor", 0);
const CHAIN = summary("chain-mail", "Chain Mail", "Armor", 55);
const TRINKET = summary("mithral-trinket", "Mithral Trinket", "Magic Armor", 1);

async function setup() {
  const api = new MockApi();
  vi.spyOn(api, "listItemCategories").mockResolvedValue([{ name: "Magic Armor", count: 1 }]);
  vi.spyOn(api, "searchItems").mockResolvedValue({ total: 2, items: [PLUS_ONE, TRINKET] });
  vi.spyOn(api, "listBaseItems").mockImplementation(async ({ itemId }) => (itemId === PLUS_ONE.id ? [CHAIN] : []));
  const add = vi.spyOn(api, "addItem").mockImplementation(async (a) => api.loadCharacter({ characterId: a.characterId }));
  const user = userEvent.setup();
  const id = await seededId(api, "Seraphine");
  renderApp("/equipment", { api, openCharacterId: id });
  return { user, add };
}

describe("adding a generic magic armor", () => {
  it("needs a base from the second list, and adds the magic item on that base", async () => {
    const { user, add } = await setup();
    const catalog = await screen.findByRole("table", { name: "Item catalog" });
    await user.click(await within(catalog).findByRole("button", { name: "Armor, +1" }));
    const bases = await screen.findByRole("table", { name: "Base items" });
    const addButton = screen.getByRole("button", { name: "Add" });
    expect(addButton).toHaveProperty("disabled", true);
    await user.click(within(bases).getByRole("button", { name: "Chain Mail" }));
    await waitFor(() => expect(addButton).toHaveProperty("disabled", false));
    await user.click(addButton);
    await waitFor(() => expect(add).toHaveBeenCalled());
    expect(add.mock.calls[0]![0]).toMatchObject({ itemId: PLUS_ONE.id, baseItemId: CHAIN.id, buy: false });
  });

  it("shows no second list for an item that needs no base", async () => {
    const { user, add } = await setup();
    const catalog = await screen.findByRole("table", { name: "Item catalog" });
    await user.click(await within(catalog).findByRole("button", { name: "Mithral Trinket" }));
    const addButton = screen.getByRole("button", { name: "Add" });
    await waitFor(() => expect(addButton).toHaveProperty("disabled", false));
    expect(screen.queryByRole("table", { name: "Base items" })).toBeNull();
    await user.click(addButton);
    await waitFor(() => expect(add).toHaveBeenCalled());
    expect(add.mock.calls[0]![0]).toMatchObject({ itemId: TRINKET.id, baseItemId: undefined });
  });
});
