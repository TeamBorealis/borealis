import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

async function openEquipment() {
  const user = userEvent.setup();
  const api = new MockApi();
  const id = await seededId(api, "Seraphine");
  renderApp("/equipment", { api, openCharacterId: id });
  return { user, api, id };
}

const headers = (grid: HTMLElement) => within(grid).getAllByRole("columnheader").map((h) => h.textContent);

describe("equipment tabs and categories", () => {
  it("has the sub-tabs EQUIPMENT and ADDITIONAL TREASURE and a CATEGORY sub-header", async () => {
    await openEquipment();
    const tabs = await screen.findAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["Equipment", "Additional treasure"]);
    const nav = screen.getByRole("navigation", { name: "Item categories" });
    expect(within(nav).getByText("Category")).toBeTruthy();
    expect(within(nav).queryByText(/^All/)).toBeNull();
  });
});

describe("equipment column sets", () => {
  it("armor: NAME, PRICE, WEIGHT, ARMOR CLASS, STRENGTH, STEALTH, GROUP, SOURCE", async () => {
    const { user } = await openEquipment();
    await user.click(await screen.findByRole("button", { name: /^Armor \(/ }));
    const grid = await screen.findByRole("table", { name: "Item catalog" });
    expect(headers(grid)).toEqual(["Name", "Price", "Weight", "Armor class", "Strength", "Stealth", "Group", "Source"]);
    const row = (await within(grid).findByRole("button", { name: "Chain Mail" })).closest("tr")!;
    for (const t of ["75 gp", "55 lb.", "16", "13", "Disadvantage", "Heavy"]) expect(within(row).getByText(t)).toBeTruthy();
    const shirt = (await within(grid).findByRole("button", { name: "Chain Shirt" })).closest("tr")!;
    expect(within(shirt).getByText("13 + Dex modifier (max 2)")).toBeTruthy();
  });

  it("packs and gear: NAME, PRICE, WEIGHT, SOURCE", async () => {
    const { user } = await openEquipment();
    await user.click(await screen.findByRole("button", { name: /^Equipment Packs \(/ }));
    const grid = await screen.findByRole("table", { name: "Item catalog" });
    await within(grid).findByRole("button", { name: /Explorer/ });
    expect(headers(grid)).toEqual(["Name", "Price", "Weight", "Source"]);
  });

  it("weapons: damage, properties, mastery and group columns", async () => {
    const { user } = await openEquipment();
    await user.click(await screen.findByRole("button", { name: /^Weapons \(/ }));
    const grid = await screen.findByRole("table", { name: "Item catalog" });
    expect(headers(grid)).toEqual(["Name", "Price", "Weight", "Damage", "Properties", "Mastery", "Group", "Source"]);
  });

  it("the inventory has no DETAILS column", async () => {
    await openEquipment();
    const inv = await screen.findByRole("region", { name: "Inventory" });
    const names = within(inv).getAllByRole("columnheader").map((h) => h.textContent);
    expect(names).not.toContain("Details");
    expect(names).toContain("Equip location");
  });
});

describe("add and buy", () => {
  it("ADD uses the inline quantity box and the status bar reports it", async () => {
    const { user, api, id } = await openEquipment();
    await user.click(await screen.findByRole("button", { name: /^Adventuring Gear \(/ }));
    await user.click(await screen.findByRole("button", { name: "Rope" }));
    const qty = screen.getByRole("textbox", { name: "Add quantity" });
    await user.clear(qty);
    await user.type(qty, "3");
    await user.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(async () => {
      const s = await api.getSnapshot({ characterId: id });
      expect(s.inventory.items.find((i) => i.itemId === "rope")?.quantity).toBe(3);
    });
    expect(await screen.findByText(/You have added 3 x 'Rope' to your inventory/i)).toBeTruthy();
  });

  it("BUY takes its own quantity box and clamps it to 1-100", async () => {
    const { user } = await openEquipment();
    await user.click(await screen.findByRole("button", { name: /^Adventuring Gear \(/ }));
    const buy = screen.getByRole("textbox", { name: "Buy quantity" }) as HTMLInputElement;
    await user.clear(buy);
    await user.type(buy, "500");
    await user.tab();
    expect(buy.value).toBe("100");
    expect((screen.getByRole("textbox", { name: "Add quantity" }) as HTMLInputElement).value).toBe("1");
  });

  it("the filter toggle tile switches the name filter off", async () => {
    const { user } = await openEquipment();
    await user.click(await screen.findByRole("button", { name: /^Adventuring Gear \(/ }));
    const grid = await screen.findByRole("table", { name: "Item catalog" });
    await within(grid).findByRole("button", { name: "Rope" });
    await user.type(screen.getByRole("textbox", { name: "Filter text" }), "torch");
    await waitFor(() => expect(within(grid).queryByRole("button", { name: "Rope" })).toBeNull());
    await user.click(screen.getByRole("button", { name: "Filter items" }));
    expect(await within(grid).findByRole("button", { name: "Rope" })).toBeTruthy();
  });
});

describe("inventory actions", () => {
  it("shows EXTRACT for a selected pack", async () => {
    const { user } = await openEquipment();
    const inv = await screen.findByRole("region", { name: "Inventory" });
    await user.click(await within(inv).findByRole("button", { name: /Select Explorer/ }));
    expect(within(inv).getByRole("button", { name: /^Extract Explorer/ })).toBeTruthy();
  });
});

describe("additional treasure sub-tab", () => {
  it("saves treasure and quest items through updateDetails", async () => {
    const { user, api, id } = await openEquipment();
    await user.click(await screen.findByRole("tab", { name: "Additional treasure" }));
    await user.type(await screen.findByLabelText("Additional treasure", { selector: "textarea" }), "A sack of gems");
    await user.tab();
    await user.type(screen.getByLabelText("Quest items", { selector: "textarea" }), "The Sunblade hilt");
    await user.tab();
    await waitFor(async () => {
      const d = (await api.getSnapshot({ characterId: id })).details;
      expect(d.treasure).toBe("A sack of gems");
      expect(d.questItems).toBe("The Sunblade hilt");
    });
  });
});

describe("equipment full-row selection", () => {
  it("selects the row when a non-name cell is pressed and enables ADD", async () => {
    const { user } = await openEquipment();
    const grid = await screen.findByRole("table", { name: "Item catalog" });
    const row = (await within(grid).findByRole("button", { name: "Backpack" })).closest("tr")!;
    const add = screen.getByRole("button", { name: /^Add$/i });
    expect((add as HTMLButtonElement).disabled).toBe(true);
    await user.click(row.querySelectorAll("td")[1]!);
    expect(row.classList.contains("sel")).toBe(true);
    expect((add as HTMLButtonElement).disabled).toBe(false);
  });
});
