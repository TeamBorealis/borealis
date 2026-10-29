import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { itemCategoryList } from "../api/mock/item-categories";
import { renderApp, seededId } from "./utils";

async function openEquipment(api = new MockApi()) {
  const user = userEvent.setup();
  const id = await seededId(api, "Seraphine");
  renderApp("/equipment", { api, openCharacterId: id });
  return { user, api, id };
}

const navLabels = () => within(screen.getByRole("navigation", { name: "Item categories" })).getAllByRole("button").map((b) => b.textContent);
const comboLabels = () => within(screen.getByRole("combobox", { name: "Category" })).getAllByRole("option").map((o) => o.textContent);

describe("equipment categories come from list_item_categories", () => {
  it("lists the non-empty categories in the classic builder's order, content categories after the fixed ones", { timeout: 15000 }, async () => {
    const api = new MockApi();
    const spy = vi.spyOn(api, "listItemCategories");
    const { id } = await openEquipment(api);
    // The mock host scopes every item for the character before it counts; give it the time a loaded machine needs.
    await screen.findByRole("button", { name: /^Magic Items \(/ }, { timeout: 5000 });
    expect(navLabels()).toEqual([
      // B-55 W3: one row per name (the sample Hooded Lantern in three sources counts once); the 2024 sample character
      // also sees the 2014-only Trident while the other edition is allowed, and the sample magic weapon Flame Tongue.
      "Adventuring Gear (7)", "Equipment Packs (2)", "Tools (2)", "Armor (5)", "Weapons (11)", "Ammunition (1)",
      "Wondrous Items (2)", "Staffs (1)", "Magic Items (5)",
      // the generated proxy categories follow the content ones (the sample wizard sees its own spell one); Feat counts
      // the mock's epic Boon of Fate since U12
      ...["Feat (8)", "Language (10)", "Proficiency (18)", "Ability Score Improvement (12)", "Spell (28)", "Wizard Spell (28)"].map((s) => `Additional ${s}`),
    ]);
    expect(comboLabels()).toEqual(navLabels());
    expect(spy).toHaveBeenCalled();
    expect(spy.mock.calls.every(([a]) => a.characterId === id)).toBe(true);
  });

  it("offers an imported category in the list and the dropdown and searches it for the character", async () => {
    const api = new MockApi();
    vi.spyOn(api, "listItemCategories").mockResolvedValue([
      { name: "Adventuring Gear", count: 6 },
      { name: "Curiosities", count: 2 },
      { name: "Additional Wizard Spell", count: 1 },
    ]);
    const search = vi.spyOn(api, "searchItems");
    const { user, id } = await openEquipment(api);
    await user.click(await screen.findByRole("button", { name: "Curiosities (2)" }));
    expect(comboLabels()).toEqual(["Adventuring Gear (6)", "Curiosities (2)", "Additional Wizard Spell (1)"]);
    await waitFor(() => expect(search).toHaveBeenCalledWith(expect.objectContaining({ category: "Curiosities", characterId: id })));
    expect(screen.getByRole("button", { name: "Curiosities (2)" }).getAttribute("aria-current")).toBe("true");
  });

  it("the rule puts Additional categories last and gates the class spell ones on spellcasting", () => {
    const items = [
      { category: "Additional Wizard Spell", visible: true },
      { category: "Curiosities", visible: true },
      { category: "Additional Spell", visible: true },
      { category: "Tools", visible: true },
      { category: "Vehicles", visible: false },
      { category: "Adventuring Gear", visible: true },
    ];
    expect(itemCategoryList(items, []).map((c) => c.name)).toEqual(["Adventuring Gear", "Tools", "Curiosities", "Additional Spell"]);
    expect(itemCategoryList(items, ["Wizard"]).map((c) => c.name)).toEqual(["Adventuring Gear", "Tools", "Curiosities", "Additional Wizard Spell", "Additional Spell"]);
  });
});
