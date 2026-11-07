import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { InventoryItem } from "../api/contract";
import { MockApi } from "../api/mock";
import { itemMenuItems } from "../pages/equipment/itemMenu";
import { renderApp, seededId } from "./utils";

const NOOP = { details: () => {}, extract: () => {}, addToAttacks: () => {}, equip: () => {}, unequip: () => {}, toggleAttunement: () => {}, remove: () => {} };

const line = (over: Partial<InventoryItem>): InventoryItem => ({
  id: "i1", itemId: "x", name: "X", displayName: "X", category: "Gear", quantity: 1, weight: 1, valueCp: 0, equipped: null, attuned: false,
  requiresAttunement: false, storage: 0, notes: "", listInInventory: true, isPack: false, packContents: [], isWeapon: false, equippable: false,
  weapon: null, armor: null, overrideRequirements: false, withheldGrants: [], ...over,
});
const weapon = (over: Partial<InventoryItem> = {}) => line({
  isWeapon: true, equippable: true, weapon: { damage: "1d8", damageType: "slashing", properties: "Versatile (1d10)", ability: "str", mastery: null, range: null }, ...over,
});
const labels = (i: InventoryItem) => itemMenuItems(i, NOOP).map((m) => `${m.separator ? "| " : ""}${m.label}`);

describe("the inventory menu entries follow Aurora's context menu (GitHub #28)", () => {
  it("lists a carried versatile weapon's entries in Aurora's order, with its separators", () => {
    expect(labels(weapon({ quantity: 2 }))).toEqual([
      "Edit Item Details", "Add to Attacks & Spellcasting",
      "| Equip", "Equip in Primary Hand", "Equip in Secondary Hand", "Equip as Versatile",
      "| Remove Item", "Remove Items (All Selected Stackables)",
    ]);
  });

  it("offers Unequip for a held weapon, Extract for a pack and attunement for an attunable item", () => {
    expect(labels(weapon({ equipped: "primary" }))).toEqual(["Edit Item Details", "Add to Attacks & Spellcasting", "| Unequip", "| Remove Item"]);
    expect(labels(line({ isPack: true }))).toEqual(["Edit Item Details", "Extract Item", "| Remove Item"]);
    expect(labels(line({ requiresAttunement: true, equippable: true }))).toEqual(["Edit Item Details", "| Equip", "| Toggle Attunement", "| Remove Item"]);
  });
});

async function withDagger() {
  const api = new MockApi({ rng: () => 0.5 });
  const id = await seededId(api, "Brannoc");
  await api.addItem({ characterId: id, itemId: "dagger", quantity: 1, buy: false });
  renderApp("/equipment", { api, openCharacterId: id });
  const inv = await screen.findByRole("region", { name: "Inventory" });
  const name = await within(inv).findByRole("button", { name: "Select Dagger" });
  return { api, id, name };
}

describe("the inventory context menu", () => {
  it("adds a weapon to the attacks from a right-click", async () => {
    // Arrange
    const { api, id, name } = await withDagger();
    const user = userEvent.setup();

    // Act
    fireEvent.contextMenu(name.closest("tr")!);
    const menu = await screen.findByRole("menu", { name: "Dagger" });
    await user.click(within(menu).getByRole("menuitem", { name: "Add to Attacks & Spellcasting" }));

    // Assert
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).attacks.some((a) => a.name === "Dagger")).toBe(true));
    expect(await screen.findByText("Dagger added to your attacks.")).toBeTruthy();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("opens from the keyboard (Shift+F10) and Edit Item Details opens the edit form", async () => {
    const { name } = await withDagger();
    const user = userEvent.setup();
    name.focus();
    await user.keyboard("{Shift>}{F10}{/Shift}");
    const menu = await screen.findByRole("menu", { name: "Dagger" });
    await user.click(within(menu).getByRole("menuitem", { name: "Edit Item Details" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText("Display name")).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("the item details (the phone's action sheet) offer Add to attacks too", async () => {
    const { api, id, name } = await withDagger();
    const user = userEvent.setup();
    await user.dblClick(name);
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Add to attacks" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).attacks.some((a) => a.name === "Dagger")).toBe(true));
  });
});
