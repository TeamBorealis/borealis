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

/** One generated proxy item of the mock (the classic builder's hidden "Additional Feat, X" items). */
export interface ProxyRow {
  id: string;
  name: string;
  category: string;
  /** Source of the target element. */
  source: string;
  keywords: string[];
}

/** What a proxy is generated from. */
export interface ProxyTarget {
  id: string;
  type: string;
  name: string;
  source: string;
  tags?: string[];
}

/** Element kinds that get a proxy, in the order of their generated categories. */
const PROXY_KINDS: [string, string][] = [["feat", "Feat"], ["language", "Language"], ["skill", "Proficiency"], ["asi", "Ability Score Improvement"], ["spell", "Spell"]];

const capitalised = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/**
 * The mock's proxy catalogue (the backend's `ProxyTable` enumeration): one row per feat, language, skill
 * proficiency, ability-score improvement and spell, then one per spellcasting class for every spell, in the
 * generated categories' order ("Additional Feat" ... "Additional Spell", then "Additional <Class> Spell").
 */
export function proxyRows(elements: ProxyTarget[], casterClassIds: string[]): ProxyRow[] {
  const row = (t: ProxyTarget, label: string, cast: string): ProxyRow => {
    const category = `${ADDITIONAL}${cast ? `${cast} ` : ""}${label}`;
    const shown = t.type === "skill" ? `Skill Proficiency (${t.name})` : t.name;
    const keywords = t.type === "spell" ? [...(cast ? [cast] : []), ...(t.tags ?? []).map(capitalised)] : [];
    return { id: `proxy:${cast.toLowerCase()}:${t.id}`, name: `${category}, ${shown}`, category, source: t.source, keywords };
  };
  const plain = PROXY_KINDS.flatMap(([type, label]) => elements.filter((e) => e.type === type).map((t) => row(t, label, "")));
  const spells = elements.filter((e) => e.type === "spell");
  return [...plain, ...casterClassIds.flatMap((c) => spells.map((t) => row(t, "Spell", capitalised(c))))];
}

/** Is this a generated proxy id of the mock? */
export const isProxyId = (id: string): boolean => id.startsWith("proxy:");

/**
 * The classic builder's name filter inside one category: `+` separates alternatives; within one, every
 * space-separated word must occur (any case) in the name or in one keyword.
 */
export function matchesItemSearch(name: string, keywords: string[], query: string): boolean {
  const hay = [name, ...keywords].map((s) => s.toLowerCase());
  const alternatives = query
    .toLowerCase()
    .split("+")
    .map((a) => a.trim().split(/\s+/).filter(Boolean))
    .filter((a) => a.length > 0);
  return alternatives.length === 0 || alternatives.some((words) => words.every((w) => hay.some((h) => h.includes(w))));
}
