import type { ItemCategory } from "../types";

/** The classic builder's fixed category order (the backend's `FIXED_CATEGORIES`). */
export const FIXED_ITEM_CATEGORIES = [
  "Adventuring Gear", "Treasure", "Trade Goods", "Equipment Packs", "Tools", "Musical Instruments", "Armor", "Magic Armor",
  "Weapons", "Magic Weapons", "Ammunition", "Spellcasting Focus", "Wondrous Items", "Supernatural Gifts", "Staffs", "Rods",
  "Wands", "Rings", "Potions", "Poison", "Scrolls", "Spell Scrolls",
];

const ADDITIONAL = "Additional ";
const classSpellGated = (c: string) => c.includes(ADDITIONAL) && c.includes(" Spell") && c !== "Additional Spell";

/**
 * The mock's `list_item_categories`: the fixed names, then the other categories in content order with the
 * "Additional " ones last; only categories holding a visible item, and an "Additional <X> Spell" one only for an <X> caster.
 */
export function itemCategoryList(items: { category: string; visible: boolean }[], casters: string[]): ItemCategory[] {
  const found = [...new Set(items.map((i) => i.category.trim()))].filter((c) => c && !FIXED_ITEM_CATEGORIES.includes(c));
  const order = [...FIXED_ITEM_CATEGORIES, ...found.filter((c) => !c.startsWith(ADDITIONAL)), ...found.filter((c) => c.startsWith(ADDITIONAL))];
  return order
    .filter((c) => !classSpellGated(c) || casters.some((k) => c === `${ADDITIONAL}${k} Spell`))
    .map((name) => ({ name, count: items.filter((i) => i.visible && i.category.trim() === name).length }))
    .filter((c) => c.count > 0);
}
