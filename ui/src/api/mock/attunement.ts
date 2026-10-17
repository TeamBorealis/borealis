import type { InventoryItem } from "../types";
import { getItem } from "./content";
import { classIdOf } from "./slots";
import type { MockCharacter } from "./state";

/** The character meets none of the item's attunement classes (the engine's `attunement_barred`). */
export function isBarred(c: MockCharacter, itemId: string): boolean {
  const by = getItem(itemId)?.attuneBy;
  if (!by) return false;
  const classes = new Set(c.classes.map((cs) => classIdOf(c, cs.slotId)));
  return !by.classes.some((id) => classes.has(id));
}

/** "Staff of Healing requires attunement by a bard, cleric, or druid" (engine `commands::equip` warning). */
export const barredMessage = (item: InventoryItem): string =>
  `${item.displayName} requires attunement ${item.attunementRestriction ?? "by someone else"}`;

/** Adds the restriction text and the barred flag the engine reports on an inventory line. */
export function withAttunementFacts(c: MockCharacter, item: InventoryItem): InventoryItem {
  const by = getItem(item.itemId)?.attuneBy;
  if (!by) return item;
  return { ...item, attunementRestriction: by.text, attunementBarred: isBarred(c, item.itemId) };
}
