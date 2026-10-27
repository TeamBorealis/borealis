import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

// User report (1.0.0): a level 1 orc could not attune an Animated Shield because the item details offered no Equip,
// and Attune said "equip the item first". The details offered Equip only for weapons and the categories "Armor" and
// "Magic Items", so 2014 magic armor ("Magic Armor"), rings, wondrous items, rods, staffs and wands had none. Equip
// now follows the engine's own equippable flag, the same one the equip command checks.

async function withItem(itemId: string) {
  const api = new MockApi({ rng: () => 0.5 });
  const id = await seededId(api, "Brannoc");
  await api.addItem({ characterId: id, itemId, quantity: 1, buy: false });
  return { api, id };
}

async function openDetails(name: string) {
  const user = userEvent.setup();
  const inv = await screen.findByRole("region", { name: "Inventory" });
  await user.dblClick(await within(inv).findByRole("button", { name: new RegExp(`Select ${name}`) }));
  return { user, dialog: await screen.findByRole("dialog") };
}

describe("Equip in the item details follows the engine (B-52)", () => {
  it("a wondrous item that needs attunement can be equipped from its details, which attunes it", async () => {
    // Arrange
    const { api, id } = await withItem("boots-of-speed");
    renderApp("/equipment", { api, openCharacterId: id });
    const { user, dialog } = await openDetails("Boots Of Speed");

    // Act
    await user.click(within(dialog).getByRole("button", { name: "Equip" }));

    // Assert
    const line = (await api.getSnapshot({ characterId: id })).inventory.items.find((i) => i.itemId === "boots-of-speed")!;
    expect(line.equipped).not.toBeNull();
    expect(line.attuned).toBe(true);
  });

  it("an item the engine cannot equip offers no Equip", async () => {
    // Arrange
    const { api, id } = await withItem("rope");
    renderApp("/equipment", { api, openCharacterId: id });

    // Act
    const { dialog } = await openDetails("Rope");

    // Assert
    expect(within(dialog).queryByRole("button", { name: "Equip" })).toBeNull();
  });

  it("an added Additional Feat line is listed and can be equipped, though it is left out of the sheet", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = new MockApi({ rng: () => 0.5 });
    const id = await seededId(api, "Brannoc");
    const proxy = (await api.searchItems({ category: "Additional Feat", limit: 5, characterId: id })).items[0]!;
    await api.addItem({ characterId: id, itemId: proxy.id, quantity: 1, buy: false });
    renderApp("/equipment", { api, openCharacterId: id });
    const inv = await screen.findByRole("region", { name: "Inventory" });

    // Act
    await user.click(await within(inv).findByRole("button", { name: `Equip ${proxy.name}` }));

    // Assert
    const line = (await api.getSnapshot({ characterId: id })).inventory.items.find((i) => i.itemId === proxy.id)!;
    expect(line.listInInventory).toBe(false);
    expect(line.equipped).not.toBeNull();
  });
});
