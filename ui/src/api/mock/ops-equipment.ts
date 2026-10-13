import type { CoinKey, EquipRequest, EquipSlot, InventoryItem } from "../types";
import { getElement, getItem } from "./content";
import { COIN_VALUE_CP, breakdown, pay, totalCp } from "./derive-inventory";
import { armorFacts } from "./facts";
import { apiError } from "./errors";
import type { MockCharacter } from "./state";

const weaponInfo = (itemId: string): InventoryItem["weapon"] => {
  const w = getItem(itemId)?.weapon;
  if (!w) return null;
  const [dice = "1d4", ...rest] = w.damage.split(" ");
  const finesse = /finesse|ammunition/i.test(w.properties);
  return { damage: dice, damageType: rest.join(" "), properties: w.properties, ability: finesse ? "dex" : "str", mastery: w.mastery ?? null, range: w.range ?? null };
};

let counter = 0;
const nextId = (): string => `inv-${Date.now().toString(36)}-${++counter}`;

export function makeItem(itemId: string, quantity: number): InventoryItem {
  const def = getItem(itemId);
  const el = getElement(itemId);
  if (!def || !el) throw apiError("not_found", `Unknown item ${itemId}`);
  return {
    id: nextId(), itemId, name: el.name, displayName: el.name, category: def.category, quantity, weight: def.weight, valueCp: def.costCp,
    equipped: null, attuned: false, requiresAttunement: !!def.attune, storage: 0, notes: "", listInInventory: true, showCard: true,
    isPack: !!def.pack, packContents: def.pack ?? [], isWeapon: !!def.weapon, weapon: weaponInfo(itemId), armor: armorFacts(def) ?? null,
  };
}

function stack(items: InventoryItem[], itemId: string, qty: number): InventoryItem[] {
  const existing = items.find((i) => i.itemId === itemId && !i.equipped && i.storage === 0 && !i.attuned);
  if (existing && !existing.requiresAttunement) return items.map((i) => (i === existing ? { ...i, quantity: i.quantity + qty } : i));
  return [...items, makeItem(itemId, qty)];
}

export function addItem(c: MockCharacter, itemId: string, qty: number, buy: boolean): MockCharacter {
  const def = getItem(itemId);
  if (!def) throw apiError("not_found", `Unknown item ${itemId}`);
  if (qty < 1 || qty > 100) throw apiError("invalid_argument", "Quantity must be 1-100.");
  let coins = c.coins;
  if (buy) {
    const paid = pay(c.coins, def.costCp * qty);
    if (!paid) throw apiError("requirement_failed", "Not enough funds.");
    coins = paid;
  }
  return { ...c, items: stack(c.items, itemId, qty), coins };
}

export function removeItem(c: MockCharacter, id: string, all: boolean): MockCharacter {
  const items = c.items.flatMap((i) => (i.id !== id ? [i] : all || i.quantity <= 1 ? [] : [{ ...i, quantity: i.quantity - 1 }]));
  return { ...c, items };
}

const patch = (items: InventoryItem[], id: string, p: Partial<InventoryItem>) => items.map((i) => (i.id === id ? { ...i, ...p } : i));

function slotFor(item: InventoryItem, req: EquipRequest): EquipSlot {
  const def = getItem(item.itemId);
  if (def?.armor) return def.armor.shield ? "secondary" : "worn";
  if (!def?.weapon) return "worn";
  if (req === "versatile" || req === "two-handed") return "two-handed";
  if (req === "primary" || req === "secondary") return req;
  return def.weapon.properties.includes("two-handed") ? "two-handed" : "primary";
}

export function equipItem(c: MockCharacter, id: string, req: EquipRequest): MockCharacter {
  const item = c.items.find((i) => i.id === id);
  if (!item) throw apiError("not_found", "No such inventory item.");
  const slot = slotFor(item, req);
  const overlaps = (s: EquipSlot | null): boolean =>
    !!s && (s === slot || (slot === "two-handed" && s !== "worn") || (s === "two-handed" && slot !== "worn"));
  const gear = (i: InventoryItem): boolean => !!(getItem(i.itemId)?.weapon || getItem(i.itemId)?.armor);
  let items = c.items.map((i) => (i.id !== id && gear(i) && gear(item) && overlaps(i.equipped) ? { ...i, equipped: null } : i));
  const warnings: string[] = [];
  let attuned = item.attuned;
  if (item.requiresAttunement && !attuned) {
    if (items.filter((i) => i.attuned).length >= c.options.attunementLimit) {
      warnings.push(`Attunement limit (${c.options.attunementLimit}) reached: ${item.displayName} was equipped unattuned.`);
    } else attuned = true;
  }
  items = patch(items, id, { equipped: slot, attuned, storage: 0 });
  return { ...c, items, warnings };
}

export function unequipItem(c: MockCharacter, id: string): MockCharacter {
  return { ...c, items: patch(c.items, id, { equipped: null }) };
}

export function setAttuned(c: MockCharacter, id: string, attuned: boolean): MockCharacter {
  const item = c.items.find((i) => i.id === id);
  if (!item) throw apiError("not_found", "No such inventory item.");
  if (!item.requiresAttunement) throw apiError("invalid_argument", "This item does not require attunement.");
  if (attuned && c.items.filter((i) => i.attuned).length >= c.options.attunementLimit) {
    throw apiError("requirement_failed", `Attunement limit (${c.options.attunementLimit}) reached.`);
  }
  return { ...c, items: patch(c.items, id, { attuned }) };
}

export function storeItem(c: MockCharacter, id: string, storage: 0 | 1 | 2): MockCharacter {
  return { ...c, items: patch(c.items, id, { storage, equipped: null, attuned: false }) };
}

export function moveItem(c: MockCharacter, id: string, dir: "up" | "down"): MockCharacter {
  const i = c.items.findIndex((x) => x.id === id);
  const j = dir === "up" ? i - 1 : i + 1;
  if (i < 0 || j < 0 || j >= c.items.length) return c;
  const items = [...c.items];
  [items[i], items[j]] = [items[j]!, items[i]!];
  return { ...c, items };
}

export function extractPack(c: MockCharacter, id: string): MockCharacter {
  const pack = c.items.find((i) => i.id === id);
  if (!pack?.isPack) throw apiError("invalid_argument", "Item is not an extractable pack.");
  let items = c.items.filter((i) => i.id !== id);
  for (const content of pack.packContents) items = stack(items, content, 1);
  return { ...c, items };
}

export function adjustCoins(c: MockCharacter, coin: CoinKey, delta: number): MockCharacter {
  if (delta >= 0 || c.coins[coin] + delta >= 0) return { ...c, coins: { ...c.coins, [coin]: c.coins[coin] + delta } };
  const paid = pay(c.coins, -delta * COIN_VALUE_CP[coin]);
  if (!paid) throw apiError("requirement_failed", "Not enough coins.");
  return { ...c, coins: paid };
}

export function convertCoins(c: MockCharacter, to: CoinKey): MockCharacter {
  const cp = totalCp(c.coins);
  if (to === "ep") {
    const ep = Math.floor(cp / 50);
    return { ...c, coins: { ...breakdown(cp - ep * 50, "sp"), ep } };
  }
  return { ...c, coins: breakdown(cp, to) };
}
