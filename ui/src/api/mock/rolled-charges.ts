import type { InventoryItem, PlayResource, RolledCharges } from "../types";
import { getElement, getItem } from "./content";
import type { ChargeDice } from "./data-types";
import { apiError } from "./errors";
import type { MockCharacter } from "./state";

/**
 * Dice-valued item charges (engine `rolled.rs`): rolled once when the item is gained, stored per inventory line and
 * resource; a line with no stored roll uses the average rounded down (`rolled: false`). The resource exists while the
 * item is equipped, like every item resource; its id is the item's element id.
 */
const key = (inventoryId: string, resourceId: string) => `${inventoryId}|${resourceId}`;

const minOf = (d: ChargeDice): number => d.count + d.bonus;
const maxOf = (d: ChargeDice): number => d.count * d.sides + d.bonus;
const averageOf = (d: ChargeDice): number => Math.floor((d.count * (d.sides + 1)) / 2 + d.bonus);

export function rollDice(d: ChargeDice, rng: () => number): number {
  let total = d.bonus;
  for (let i = 0; i < d.count; i++) total += 1 + Math.min(d.sides - 1, Math.floor(rng() * d.sides));
  return total;
}

const chargesName = (item: InventoryItem): string => `${getElement(item.itemId)?.name ?? item.name} charges`;

/** The `InventoryItem.rolledCharges` of one line (empty while it is not equipped or has no dice-valued charges). */
export function rolledChargesOf(c: MockCharacter, item: InventoryItem): RolledCharges[] {
  const dice = getItem(item.itemId)?.charges;
  if (!dice || !item.equipped) return [];
  const stored = c.rolledMax?.[key(item.id, item.itemId)];
  return [{
    resourceId: item.itemId, name: chargesName(item), dice: dice.dice, value: stored ?? averageOf(dice),
    rolled: stored !== undefined, min: minOf(dice), max: maxOf(dice),
  }];
}

/** Adds `rolledCharges` to a line that has some (the DTO skips the field when empty). */
export function withRolledCharges(c: MockCharacter, item: InventoryItem): InventoryItem {
  const charges = rolledChargesOf(c, item);
  return charges.length ? { ...item, rolledCharges: charges } : item;
}

const store = (c: MockCharacter, inventoryId: string, resourceId: string, value: number): MockCharacter =>
  ({ ...c, rolledMax: { ...c.rolledMax, [key(inventoryId, resourceId)]: value } });

/** Rolls the charges of every line `after` gained over `before` (engine `add_item` rolls once per new line). */
export function rollGained(before: MockCharacter, after: MockCharacter, rng: () => number): MockCharacter {
  const known = new Set(before.items.map((i) => i.id));
  return after.items.reduce((c, i) => {
    const dice = getItem(i.itemId)?.charges;
    return dice && !known.has(i.id) ? store(c, i.id, i.itemId, rollDice(dice, rng)) : c;
  }, after);
}

function chargesFor(c: MockCharacter, inventoryId: string, resourceId: string): { item: InventoryItem; dice: ChargeDice } {
  const item = c.items.find((i) => i.id === inventoryId);
  if (!item) throw apiError("not_found", `No inventory line ${inventoryId}`);
  const dice = getItem(item.itemId)?.charges;
  if (!dice || resourceId !== item.itemId) throw apiError("invalid_argument", `No rolled charges ${resourceId} on ${inventoryId}`);
  return { item, dice };
}

/** Enter the maximum: refused outside min..max with the engine's wording. */
export function setRolledMax(c: MockCharacter, inventoryId: string, resourceId: string, value: number): MockCharacter {
  const { dice } = chargesFor(c, inventoryId, resourceId);
  if (!Number.isInteger(value) || value < minOf(dice) || value > maxOf(dice)) {
    throw apiError("invalid_argument", `${value} is outside ${minOf(dice)}..${maxOf(dice)} for ${resourceId}`);
  }
  return store(c, inventoryId, resourceId, value);
}

export function rerollMax(c: MockCharacter, inventoryId: string, resourceId: string, rng: () => number): MockCharacter {
  const { dice } = chargesFor(c, inventoryId, resourceId);
  return store(c, inventoryId, resourceId, rollDice(dice, rng));
}

/** The play resources of equipped items with dice-valued charges (current filled in by `buildPlay`). */
export function chargeResources(c: MockCharacter): PlayResource[] {
  const seen = new Set<string>();
  return c.items.flatMap((i) => rolledChargesOf(c, i).flatMap((r) => {
    if (seen.has(r.resourceId)) return [];
    seen.add(r.resourceId);
    return [{ id: r.resourceId, name: r.name, max: r.value, current: r.value, source: i.displayName, recharge: ["dawn"], shortRestRegain: null, longRestRegainAll: false }];
  }));
}
