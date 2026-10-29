import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Attack, ContentPack, InventoryItem } from "../api/contract";
import { MockApi } from "../api/mock";
import { AttackTags } from "../pages/manage/AttacksTab";
import { packOrigin } from "../pages/content/PackGroups";
import { attunementWarnings, restrictionText, weaponsFiring } from "../pages/equipment/attunement";
import { renderApp, seededId } from "./utils";

const RESTRICTION = "Requires attunement by a bard, cleric, or druid";

async function withItem(prefix: string, itemId: string) {
  const api = new MockApi({ rng: () => 0.5 });
  const id = await seededId(api, prefix);
  const snap = await api.addItem({ characterId: id, itemId, quantity: 1, buy: false });
  const line = snap.inventory.items.find((i) => i.itemId === itemId)!;
  return { api, id, line };
}

const item = (over: Partial<InventoryItem>): InventoryItem => ({
  id: "inv-1", itemId: "x", name: "Moonblade", displayName: "Moonblade", category: "Magic Items", quantity: 1, weight: 3, valueCp: 0,
  equipped: null, attuned: false, requiresAttunement: true, storage: 0, notes: "", listInInventory: true, isPack: false, packContents: [],
  isWeapon: false, equippable: true, weapon: null, armor: null, overrideRequirements: false, withheldGrants: [], ...over,
});
const attack = (over: Partial<Attack>): Attack => ({
  id: "a", name: "Longbow", range: "150/600", attackBonus: "+6", damage: "1d8+4 piercing", description: "", ability: "dex", auto: true, showInCombat: true, card: false, ...over,
});

describe("attunement restriction (engine attunement_restriction / attunement_barred)", () => {
  it("shows the restriction on a barred line, disables Attune and warns when equipping leaves it unattuned", async () => {
    // Arrange: a fighter carries a staff only a bard, cleric or druid can attune.
    const user = userEvent.setup();
    const { api, id } = await withItem("Brannoc", "staff-of-healing");
    renderApp("/equipment", { api, openCharacterId: id });
    const inv = await screen.findByRole("region", { name: "Inventory" });
    const row = (await within(inv).findByRole("button", { name: /Select Staff Of Healing/ })).closest("tr")!;

    // Assert: restriction text on the row, Attune disabled.
    expect(within(row).getByText(RESTRICTION)).toBeTruthy();
    expect((within(row).getByRole("button", { name: "Attune to Staff Of Healing" }) as HTMLButtonElement).disabled).toBe(true);

    // Act: equip it.
    await user.click(within(row).getByRole("button", { name: "Equip Staff Of Healing" }));

    // Assert: the engine's equip warning, still unattuned.
    expect(await within(inv).findByText("Staff Of Healing requires attunement by a bard, cleric, or druid")).toBeTruthy();
    const snap = await api.getSnapshot({ characterId: id });
    const line = snap.inventory.items.find((i) => i.itemId === "staff-of-healing")!;
    expect(line.equipped).not.toBeNull();
    expect(line.attuned).toBe(false);
  });

  it("the item details name the restriction and keep Attune disabled; a qualifying class sees no warning", async () => {
    // Arrange
    const user = userEvent.setup();
    const { api, id } = await withItem("Brannoc", "staff-of-healing");
    renderApp("/equipment", { api, openCharacterId: id });
    const inv = await screen.findByRole("region", { name: "Inventory" });
    await user.dblClick(await within(inv).findByRole("button", { name: /Select Staff Of Healing/ }));

    // Assert
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/Requires attunement by a bard, cleric, or druid\. You do not meet this requirement\./)).toBeTruthy();
    expect((within(dialog).getByRole("button", { name: "Attune" }) as HTMLButtonElement).disabled).toBe(true);
    await expect(api.setAttuned({ characterId: id, inventoryId: (await api.getSnapshot({ characterId: id })).inventory.items.find((i) => i.itemId === "staff-of-healing")!.id, attuned: true }))
      .rejects.toMatchObject({ code: "requirement_failed", message: "Staff Of Healing requires attunement by a bard, cleric, or druid" });
  });

  it("an already attuned barred item (Aurora save) shows the attunement.restricted warning in the inventory and on the sheet", () => {
    // Arrange
    const barred = item({ attuned: true, attunementBarred: true, attunementRestriction: "by an elf or half-elf" });
    const notice = "Moonblade is attuned although it requires attunement by an elf or half-elf";
    const inventory = { items: [barred, item({ id: "inv-2", name: "Ring", displayName: "Ring", attunementRestriction: "by a wizard" })] };

    // Act
    const fromItems = attunementWarnings({ inventory: inventory as never, notices: [] });
    const both = attunementWarnings({ inventory: inventory as never, notices: [notice, "Something else"] });

    // Assert: one message per attuned barred item, the engine notice not repeated.
    expect(fromItems).toEqual([notice]);
    expect(both).toEqual([notice]);
    expect(restrictionText(item({ attunementRestriction: " by a cleric " }))).toBe("Requires attunement by a cleric");
    expect(restrictionText(item({}))).toBeNull();
  });

  it("renders the attunement.restricted notice on the Character Sheet page", async () => {
    // Arrange: the engine reports the notice for an Aurora save.
    const api = new MockApi({ rng: () => 0.5 });
    const id = await seededId(api, "Brannoc");
    const real = api.loadCharacter.bind(api);
    const notice = "Staff Of Healing is attuned although it requires attunement by a bard, cleric, or druid";
    vi.spyOn(api, "loadCharacter").mockImplementation(async (a) => ({ ...(await real(a)), notices: [notice] }));

    // Act
    renderApp("/sheet", { api, openCharacterId: id });

    // Assert
    expect(await screen.findByText(notice)).toBeTruthy();
  });
});

describe("attack line: per-weapon critical range and +N ammunition", () => {
  it("shows the crit range and the ammunition only on the weapon they apply to", () => {
    // Arrange / Act
    render(<><AttackTags attack={attack({ id: "bow", critRange: 19, ammunition: "+1 Arrows (20)" })} /><div data-testid="sword"><AttackTags attack={attack({ id: "sw", name: "Longsword", critRange: 20 })} /></div></>);

    // Assert
    expect(screen.getAllByText("Crit 19-20")).toHaveLength(1);
    expect(screen.getByText("Ammunition: +1 Arrows (20)")).toBeTruthy();
    expect(within(screen.getByTestId("sword")).queryByText(/Crit|Ammunition/)).toBeNull();
  });

  it("names the weapons that fire an ammunition line", () => {
    const arrows = item({ name: "Arrow +1", displayName: "+1 Arrows (20)", requiresAttunement: false });
    const attacks = [attack({ ammunition: "+1 Arrows (20)" }), attack({ id: "s", name: "Shortbow", ammunition: "+1 Arrows (20)" }), attack({ id: "x", name: "Dagger" })];
    expect(weaponsFiring(arrows, attacks)).toEqual(["Longbow", "Shortbow"]);
    expect(weaponsFiring(item({ displayName: "Rope" }), attacks)).toEqual([]);
  });
});

describe("speed-multiply toggle (Boots of Speed)", () => {
  it("lists the item state with its speed effect and doubles the walking speed when switched on", async () => {
    // Arrange: equip (and so attune) the boots.
    const user = userEvent.setup();
    const { api, id, line } = await withItem("Seraphine", "boots-of-speed");
    await api.equipItem({ characterId: id, inventoryId: line.id, slot: "auto" });
    renderApp("/play", { api, openCharacterId: id });
    const states = await screen.findByRole("list", { name: "States" });
    expect(within(states).getByText("Walk speed x2")).toBeTruthy();
    expect((await screen.findByTestId("explain-speed-value")).textContent).toBe("30 ft");

    // Act
    await user.click(within(states).getByRole("switch", { name: "Boots Of Speed" }));

    // Assert
    await waitFor(() => expect(screen.getByTestId("explain-speed-value").textContent).toBe("60 ft"));
  });
});

describe("installed packs grouped by origin on phones", () => {
  const pack = (id: string, kind: ContentPack["kind"], extra: Partial<ContentPack> = {}): ContentPack => ({
    id, name: id, version: "1", kind, rulesets: ["dnd2014"], elementCount: 1, enabled: true, license: "x", attribution: "", sourceIds: [id], ...extra,
  });
  const packs = [
    pack("srd", "bundled"), pack("aurora-core", "downloaded"), pack("my-xml", "imported"), pack("ai-doc", "imported", { redistributable: false }),
    ...Array.from({ length: 93 }, (_, i) => pack(`5et-${i}`, "local-import", { redistributable: false })),
  ];

  it("classifies packs by origin", () => {
    expect(packs.slice(0, 5).map(packOrigin)).toEqual(["bundled", "aurora-index", "custom", "ai", "5etools"]);
    expect(packOrigin(pack("hb", "homebrew"))).toBe("custom");
  });

  it("phones: collapsed groups with counts keep the 5etools import close; a group opens on tap", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = new MockApi({ rng: () => 0.5, mobile: true });
    vi.spyOn(api, "listPacks").mockResolvedValue(packs);
    renderApp("/content", { api });

    // Assert: five collapsed groups in origin order, no pack rows rendered.
    const groups = await screen.findByRole("group", { name: "Content packs by origin" });
    const heads = within(groups).getAllByRole("button", { expanded: false }).map((b) => b.textContent);
    expect(heads).toEqual(["Built-in (1)", "Aurora index (1)", "5etools (93)", "Custom (1)", "AI import (1)"]);
    expect(screen.queryByRole("checkbox", { name: "Enable 5et-0" })).toBeNull();
    expect(screen.getByRole("textbox", { name: "5etools data address" })).toBeTruthy();

    // Act
    await user.click(within(groups).getByRole("button", { name: "5etools (93)" }));

    // Assert
    expect(within(await screen.findByRole("list", { name: "5etools packs" })).getAllByRole("listitem")).toHaveLength(93);
  });

  it("desktop keeps the flat pack list", async () => {
    const api = new MockApi({ rng: () => 0.5 });
    vi.spyOn(api, "listPacks").mockResolvedValue(packs);
    renderApp("/content", { api });
    expect(within(await screen.findByRole("list", { name: "Content packs" })).getAllByRole("listitem")).toHaveLength(packs.length);
    expect(screen.queryByRole("group", { name: "Content packs by origin" })).toBeNull();
  });
});
