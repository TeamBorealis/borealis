import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { equipmentTabs } from "../pages/equipment/EquipmentPage";
import { renderApp, seededId } from "./utils";

// GitHub #33 / B-195 (suggested by Egorobi on Discord): on a phone the inventory sat below the whole item catalogue, so
// every visit meant scrolling past it. Phones get an Inventory sub-tab beside Equipment (the catalogue) and Additional
// treasure, in the same sub-tab strip. Desktop and tablet shells keep Aurora's two sub-tabs and one screen (U7).
// jsdom has no layout, so a "390 px phone" is the phone layout flag, as in the other phone tests (b159, b48-*).

function asPhone() {
  document.documentElement.classList.add("phone");
  setPhoneForTests(true);
}

afterEach(() => {
  setPhoneForTests(false);
  document.documentElement.classList.remove("phone");
});

/** Brannoc with a dagger in the pack (or with an empty inventory), opened on the Gear screen. */
async function openGear(opts: { empty?: boolean } = {}) {
  const api = new MockApi({ rng: () => 0.5 });
  const id = await seededId(api, "Brannoc");
  if (opts.empty) {
    for (const i of (await api.getSnapshot({ characterId: id })).inventory.items) await api.removeItem({ characterId: id, inventoryId: i.id, all: true });
  } else {
    await api.addItem({ characterId: id, itemId: "dagger", quantity: 1, buy: false });
  }
  renderApp("/equipment", { api, openCharacterId: id });
  const strip = await screen.findByRole("tablist", { name: "Equipment sections" });
  return { api, id, strip, user: userEvent.setup() };
}

const tabNames = (strip: HTMLElement) => within(strip).getAllByRole("tab").map((t) => t.textContent);
const selectedTab = (strip: HTMLElement) => within(strip).getAllByRole("tab").find((t) => t.getAttribute("aria-selected") === "true")?.textContent;
const layoutParts = () => [...(document.querySelector(".eq-layout")?.children ?? [])].map((c) => c.classList[0]);

describe("Phone Gear sub-tabs (GitHub #33)", () => {
  it("on a phone the sub-tabs are Equipment, Inventory and Additional treasure, and a character with items opens on Inventory", async () => {
    // Arrange
    asPhone();

    // Act
    const { strip } = await openGear();

    // Assert
    expect(tabNames(strip)).toEqual(["Equipment", "Inventory", "Additional treasure"]);
    expect(selectedTab(strip)).toBe("Inventory");
  });

  it("the Inventory sub-tab shows the inventory and its toolbar without the catalogue", async () => {
    // Arrange
    asPhone();

    // Act
    await openGear();

    // Assert
    const inv = await screen.findByRole("region", { name: "Inventory" });
    expect(within(inv).getByRole("button", { name: "Select Dagger" })).toBeTruthy();
    expect(within(inv).getByRole("toolbar", { name: "Inventory actions" }).className).toBe("eq-inv-actions eq-pin");
    expect(layoutParts()).toEqual(["eq-inv"]);
    expect(screen.queryByRole("table", { name: "Item catalog" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Add" })).toBeNull();
    expect(screen.queryByRole("textbox", { name: "Filter text" })).toBeNull();
  });

  it("the Equipment sub-tab shows the catalogue and its tools without the inventory", async () => {
    // Arrange
    asPhone();
    const { strip, user } = await openGear();

    // Act
    await user.click(within(strip).getByRole("tab", { name: "Equipment" }));

    // Assert
    expect(await screen.findByRole("table", { name: "Item catalog" })).toBeTruthy();
    expect(layoutParts()).toEqual(["eq-cats", "eq-items", "eq-cmds", "eq-tools"]);
    expect(screen.queryByRole("region", { name: "Inventory" })).toBeNull();
  });

  it("a character with nothing in the inventory opens on the Equipment catalogue", async () => {
    // Arrange
    asPhone();

    // Act
    const { strip } = await openGear({ empty: true });

    // Assert
    expect(selectedTab(strip)).toBe("Equipment");
    expect(await screen.findByRole("table", { name: "Item catalog" })).toBeTruthy();
  });

  it("an item added from the catalogue is reported in the status bar and is on the Inventory sub-tab", async () => {
    // Arrange
    asPhone();
    const { api, id, strip, user } = await openGear({ empty: true });
    await user.click(await screen.findByRole("button", { name: "Rope" }));
    const add = screen.getByRole("button", { name: "Add" }) as HTMLButtonElement;
    await waitFor(() => expect(add.disabled).toBe(false));

    // Act
    await user.click(add);
    expect(await screen.findByText(/You have added 1 x 'Rope' to your inventory/i)).toBeTruthy();
    await user.click(within(strip).getByRole("tab", { name: "Inventory" }));

    // Assert
    const inv = await screen.findByRole("region", { name: "Inventory" });
    expect(within(inv).getByRole("button", { name: "Select Rope" })).toBeTruthy();
    expect((await api.getSnapshot({ characterId: id })).inventory.items.some((i) => i.itemId === "rope")).toBe(true);
  });

  it("the catalogue keeps its category while the user flips to the inventory and back", async () => {
    // Arrange
    asPhone();
    const { strip, user } = await openGear();
    await user.click(within(strip).getByRole("tab", { name: "Equipment" }));
    const category = (await screen.findByRole("combobox", { name: "Category" })) as HTMLSelectElement;
    const other = [...category.options].map((o) => o.value).find((v) => v !== category.value)!;
    fireEvent.change(category, { target: { value: other } });

    // Act
    await user.click(within(strip).getByRole("tab", { name: "Inventory" }));
    await user.click(within(strip).getByRole("tab", { name: "Equipment" }));

    // Assert
    expect(((await screen.findByRole("combobox", { name: "Category" })) as HTMLSelectElement).value).toBe(other);
  });

  it("the inventory context menu still adds a weapon to the attacks on the Inventory sub-tab", async () => {
    // Arrange
    asPhone();
    const { api, id, user } = await openGear();
    const inv = await screen.findByRole("region", { name: "Inventory" });

    // Act
    fireEvent.contextMenu(within(inv).getByRole("button", { name: "Select Dagger" }).closest("tr")!);
    const menu = await screen.findByRole("menu", { name: "Dagger" });
    await user.click(within(menu).getByRole("menuitem", { name: "Add to Attacks & Spellcasting" }));

    // Assert
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).attacks.some((a) => a.name === "Dagger")).toBe(true));
    expect(await screen.findByText("Dagger added to your attacks.")).toBeTruthy();
  });

  it("leaving the phone layout (a tablet turned to the wide shell) folds Inventory back into the classic Equipment screen", async () => {
    // Arrange
    asPhone();
    const { strip } = await openGear();
    expect(selectedTab(strip)).toBe("Inventory");

    // Act
    act(() => setPhoneForTests(false));

    // Assert
    const desk = await screen.findByRole("tablist", { name: "Equipment sections" });
    expect(tabNames(desk)).toEqual(["Equipment", "Additional treasure"]);
    expect(selectedTab(desk)).toBe("Equipment");
    expect(layoutParts()).toEqual(["eq-cats", "eq-items", "eq-tools", "eq-inv"]);
  });
});

describe("Desktop Gear screen is unchanged (GitHub #33, U7)", () => {
  it("keeps Aurora's two sub-tabs, opens on Equipment, and shows catalogue, tools and inventory on one screen", async () => {
    // Act
    const { strip } = await openGear();

    // Assert
    expect(tabNames(strip)).toEqual(["Equipment", "Additional treasure"]);
    expect(selectedTab(strip)).toBe("Equipment");
    expect(await screen.findByRole("table", { name: "Item catalog" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Inventory" })).toBeTruthy();
    expect(layoutParts()).toEqual(["eq-cats", "eq-items", "eq-tools", "eq-inv"]);
    expect(document.querySelector(".eq-pin")).toBeNull();
  });

  it("the tab list adds Inventory only for the phone layout", () => {
    expect(equipmentTabs(false).map((t) => t.id)).toEqual(["equipment", "treasure"]);
    expect(equipmentTabs(true).map((t) => t.id)).toEqual(["equipment", "inventory", "treasure"]);
  });
});
