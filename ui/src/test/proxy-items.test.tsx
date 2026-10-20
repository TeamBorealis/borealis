import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ItemSummary } from "../api/contract";
import { MockApi } from "../api/mock";
import { matchesItemSearch } from "../api/mock/item-categories";
import { renderApp, seededId } from "./utils";

/** The classic builder's hidden "Additional ..." items (specs/proxy-items.md P5). */
const SPELLS = "Additional Spell";
const BIG = 250;

const row = (n: number): ItemSummary => ({
  id: `proxy-${n}`, name: `${SPELLS}, Spell ${String(n).padStart(3, "0")}`, type: "item", source: "sample", sourceName: "Sample",
  shortDescription: "", prerequisites: null, rulesets: ["dnd2014", "dnd2024"], category: SPELLS, costCp: 0, weight: 0, requiresAttunement: false,
});

async function openEquipment(api: MockApi) {
  const user = userEvent.setup();
  const id = await seededId(api, "Seraphine");
  renderApp("/equipment", { api, openCharacterId: id });
  return { user, id };
}

describe("the item grid reaches every row of a category", () => {
  it("pages through a category of more than 200 items, so its last item is listed", async () => {
    const api = new MockApi();
    vi.spyOn(api, "listItemCategories").mockResolvedValue([{ name: SPELLS, count: BIG }]);
    const all = Array.from({ length: BIG }, (_, i) => row(i + 1));
    const search = vi.spyOn(api, "searchItems").mockImplementation(async (q) => {
      const from = q.offset ?? 0;
      return { total: all.length, items: all.slice(from, from + Math.min(q.limit ?? 100, 100)) };
    });
    await openEquipment(api);
    const grid = await screen.findByRole("table", { name: "Item catalog" });
    expect(await within(grid).findByRole("button", { name: `${SPELLS}, Spell ${BIG}` })).toBeTruthy();
    expect(within(grid).getAllByRole("row")).toHaveLength(BIG + 1);
    expect(search.mock.calls.some(([q]) => (q.offset ?? 0) > 0)).toBe(true);
  });
});

describe("the name filter inside a category follows the classic builder", () => {
  it("needs every word in the name or one keyword; + separates alternatives", () => {
    const kw = ["Wizard", "Evocation"];
    expect(matchesItemSearch("Additional Spell, Fire Bolt", kw, "fire BOLT")).toBe(true);
    expect(matchesItemSearch("Additional Spell, Fire Bolt", kw, "fire wizard")).toBe(true);
    expect(matchesItemSearch("Additional Spell, Fire Bolt", kw, "fire cleric")).toBe(false);
    expect(matchesItemSearch("Additional Spell, Fire Bolt", kw, "cleric + bolt")).toBe(true);
    expect(matchesItemSearch("Additional Spell, Fire Bolt", kw, "  ")).toBe(true);
  });

  it("the mock search finds proxies only inside their category", async () => {
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    const inside = await api.searchItems({ category: SPELLS, query: "wizard", limit: 1000, characterId: id });
    expect(inside.total).toBeGreaterThan(0);
    expect(inside.items.every((i) => i.name.startsWith(`${SPELLS}, `))).toBe(true);
    const anywhere = await api.searchItems({ query: "additional", limit: 1000, characterId: id });
    expect(anywhere.items).toHaveLength(0);
    const detail = await api.getElement({ elementId: inside.items[0]!.id });
    expect(detail.descriptionHtml.length).toBeGreaterThan(0);
  });
});

describe("adding a generated proxy item (IM13, specs/proxy-items.md P4)", () => {
  async function setup() {
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    const listed = await api.searchItems({ category: SPELLS, limit: 1000, characterId: id });
    return { api, id, proxy: listed.items[0]! };
  }

  it("adds an id the listing offers as a free, hidden line without a card", async () => {
    const { api, id, proxy } = await setup();
    const start = await api.loadCharacter({ characterId: id });
    const snap = await api.addItem({ characterId: id, itemId: proxy.id, quantity: 1, buy: true });
    const line = snap.inventory.items.find((i) => i.itemId === proxy.id)!;
    expect(snap.inventory.items).toHaveLength(start.inventory.items.length + 1);
    expect(line.displayName).toBe(proxy.name);
    expect(line.category).toBe(SPELLS);
    expect(line.listInInventory).toBe(false);
    expect(line.showCard).toBe(false);
    expect(snap.inventory.coins).toEqual(start.inventory.coins);
  });

  it("never stacks: a second add is a line of its own", async () => {
    const { api, id, proxy } = await setup();
    await api.addItem({ characterId: id, itemId: proxy.id, quantity: 1, buy: false });
    const snap = await api.addItem({ characterId: id, itemId: proxy.id, quantity: 1, buy: false });
    expect(snap.inventory.items.filter((i) => i.itemId === proxy.id)).toHaveLength(2);
  });

  it("still rejects an id the listing does not offer", async () => {
    const { api, id } = await setup();
    await expect(api.addItem({ characterId: id, itemId: "proxy::nothing", quantity: 1, buy: false })).rejects.toMatchObject({ code: "not_found" });
  });

  it("leaves hidden lines out of the sheet's gear list", async () => {
    const { api, id, proxy } = await setup();
    await api.addItem({ characterId: id, itemId: proxy.id, quantity: 1, buy: false });
    const after = (await api.renderSheet({ characterId: id })).html;
    expect(after).not.toContain(proxy.name);
  });
});
