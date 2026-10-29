import type { Attack, CharacterSnapshot, OptionDef, SelectionProgress, SelectionSlot, BuilderTab } from "../types";
import { getElement, getItem, titleCase } from "./content";
import { deriveAbilities } from "./derive-abilities";
import { deriveInventory } from "./derive-inventory";
import { deriveSpellcasting } from "./derive-spells";
import { deriveLevel, deriveStats, profBonus } from "./derive-stats";
import { buildPlay } from "./play";
import { type SlotDef, classIdOf, reprocess } from "./slots";
import { isEquippable } from "./ops-equipment";
import { toWireSlotId } from "./slot-ids";
import { type MockCharacter, type RuleSettings, signed, totalLevel } from "./state";

/** Content options as the backend lists them (`list_character_options`; the three built-ins are not among them). */
export const OPTION_DEFS: OptionDef[] = [
  { id: "ID_MOCK_OPTION_CUSTOMIZED_LANGUAGE", name: "Customized Language", description: "You may replace each language in your Languages trait with another standard language.", locked: false, defaultOn: false },
  { id: "ID_MOCK_OPTION_FIREARMS", name: "Firearms", description: "You can introduce gunpowder, futuristic, or modern firearms to your campaign.", locked: false, defaultOn: true },
];

const TABS: BuilderTab[] = ["race", "class", "background", "abilities", "feats", "languages", "proficiencies", "companion", "spells"];

export const toSlot = (d: SlotDef, c: MockCharacter): SelectionSlot => {
  const chosenId = c.choices[d.id];
  const el = chosenId ? getElement(chosenId) : undefined;
  const { candidates: _candidates, dup: _dup, lockedTo: _lockedTo, ...rest } = d;
  // engine-optional-sole-option SO-3: only a defaulted sole option (a fixed grant here) is locked
  const locked = d.locked;
  const history = c.slotHistory?.[d.id] ?? [];
  const replaced = history.map((h) => ({ id: h.id, name: getElement(h.id)?.name ?? h.id, level: h.level }));
  const gainedOnLevel = history.at(-1)?.level ?? d.requiredLevel ?? 1;
  return {
    ...rest, id: toWireSlotId(d.id), locked, gainedOnLevel, replaced, allowReplace: d.allowReplace && !!el,
    chosen: el ? { id: el.id, name: el.name, type: el.type, source: el.source } : null,
  };
};

function progress(slots: SelectionSlot[]): SelectionProgress {
  const byTab = Object.fromEntries(TABS.map((t) => [t, 0])) as Record<BuilderTab, number>;
  const required = slots.filter((s) => s.required);
  const pending = required.filter((s) => !s.chosen);
  for (const s of pending) byTab[s.tab] += 1;
  const order: BuilderTab[] = ["race", "class", "background", "abilities", "languages", "proficiencies", "feats", "companion", "spells"];
  const next = [...pending].sort((a, b) => order.indexOf(a.tab) - order.indexOf(b.tab))[0];
  return { total: required.length, remaining: pending.length, nextSlotId: next?.id ?? null, byTab };
}

/** Carried +N ammunition a weapon attack fires (engine `AttackSnapshot.ammunition`): the line's name and bonus. */
function ammunitionFor(c: MockCharacter, a: Attack): { name: string; bonus: number } | null {
  const weapon = c.items.find((i) => i.displayName === a.name && getItem(i.itemId)?.weapon);
  if (!weapon || !/ammunition/i.test(getItem(weapon.itemId)?.weapon?.properties ?? "")) return null;
  const ammo = c.items.find((i) => i.storage === 0 && getItem(i.itemId)?.ammoBonus);
  return ammo ? { name: ammo.displayName, bonus: getItem(ammo.itemId)?.ammoBonus ?? 0 } : null;
}

function resolveAttacks(c: MockCharacter, mods: Record<string, number>, prof: number): Attack[] {
  return c.attacks.map(({ ammunition: _stale, ...a }) => {
    if (!a.auto || !a.ability) return a;
    const ammo = ammunitionFor(c, a);
    const m = (mods[a.ability] ?? 0) + (ammo?.bonus ?? 0);
    const resolved = { ...a, attackBonus: signed(m + prof), damage: a.damage.replace(/([+-]\d+)?$/, "") + (m ? signed(m) : "") };
    return ammo ? { ...resolved, ammunition: ammo.name } : resolved;
  });
}

export function buildName(c: MockCharacter): string {
  const race = getElement(c.choices["race:0"] ?? "")?.name;
  const classes = c.classes
    .map((cs) => {
      const id = classIdOf(c, cs.slotId);
      const lvl = c.levelOrder.filter((x) => x === cs.slotId).length;
      return id ? `${titleCase(id)} ${lvl}` : null;
    })
    .filter(Boolean)
    .join(" / ");
  return [race, classes || `Level ${totalLevel(c)}`].filter(Boolean).join(" ");
}

export function buildSnapshot(input: MockCharacter, rules: RuleSettings): { character: MockCharacter; snapshot: CharacterSnapshot } {
  const { character: c, defs } = reprocess(input);
  const abilities = deriveAbilities(c, rules);
  const slots = defs.map((d) => toSlot(d, c));
  const stats = deriveStats(c, abilities);
  const mods = abilities.modifier;
  const items = c.items.map((i) => ({ ...i, isWeapon: !!getItem(i.itemId)?.weapon, equippable: i.equippable ?? isEquippable(getItem(i.itemId)) }));
  const inv = deriveInventory({ ...c, items }, abilities);
  const snapshot: CharacterSnapshot = {
    id: c.id, name: c.name, ruleset: c.ruleset, portrait: c.portrait, group: c.group, favorite: c.favorite, dirty: c.dirty, displayPath: c.path ?? "",
    level: deriveLevel(c), abilities, stats, slots, progress: progress(slots),
    spellcasting: deriveSpellcasting(c, abilities), inventory: inv, details: c.details,
    attacks: resolveAttacks(c, mods, profBonus(totalLevel(c))),
    companion: null, options: c.options,
    availableOptions: OPTION_DEFS, sourceRestrictions: c.sourceRestrictions, notices: c.notices,
    play: buildPlay(c),
  };
  return { character: c, snapshot };
}
