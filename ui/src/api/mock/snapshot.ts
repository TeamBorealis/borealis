import { ABILITY_KEYS, type Attack, type CharacterSnapshot, type OptionDef, type SelectionProgress, type SelectionSlot, type BuilderTab } from "../types";
import { getElement, getItem, titleCase } from "./content";
import { buildCompanions } from "./companions";
import { deriveAbilities } from "./derive-abilities";
import { deriveInventory } from "./derive-inventory";
import { deriveSpellcasting } from "./derive-spells";
import { deriveLevel, deriveStats, profBonus } from "./derive-stats";
import { buildPlay } from "./play";
import { type SlotDef, classIdOf, reprocess } from "./slots";
import { isEquippable } from "./ops-equipment";
import { emptyReasonOf } from "./edition-classes";
import { toWireSlotId } from "./slot-ids";
import { type MockCharacter, type RuleSettings, d20Tests, signed, totalLevel } from "./state";

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
    ...rest, id: toWireSlotId(d.id), locked, gainedOnLevel, replaced, allowReplace: d.allowReplace && !!el, emptyReason: emptyReasonOf(d, c), problem: null,
    chosen: el ? { id: el.id, name: el.name, type: el.type, source: el.source } : null,
  };
};

/** Selection Flow group order (host `snapshot_map::FLOW_TABS`, Aurora's Race -> Class -> Background -> ... -> Spell). */
const FLOW_TABS: BuilderTab[] = ["race", "class", "background", "abilities", "languages", "proficiencies", "feats", "companion", "spells"];
/** Host `ids::flow_priority` by UI element type; unlisted types rank last. */
const FLOW_PRIORITY: Record<string, number> = {
  race: 0, subrace: 1, class: 2, archetype: 3, multiclass: 4, background: 5, asi: 6, language: 7, proficiency: 8, skill: 8, feat: 9, companion: 10, spell: 11,
};
const UNRANKED_FLOW_PRIORITY = 12;
const flowPriority = (s: SelectionSlot): number => FLOW_PRIORITY[s.elementType] ?? UNRANKED_FLOW_PRIORITY;

function progress(slots: SelectionSlot[]): SelectionProgress {
  const byTab = Object.fromEntries(TABS.map((t) => [t, 0])) as Record<BuilderTab, number>;
  const required = slots.filter((s) => s.required);
  const pending = required.filter((s) => !s.chosen);
  for (const s of pending) byTab[s.tab] += 1;
  // A10: the host's keys: Aurora's group (tab) order, then the selected type's flow priority, then slot order (a stable sort).
  const next = [...pending].sort((a, b) => FLOW_TABS.indexOf(a.tab) - FLOW_TABS.indexOf(b.tab) || flowPriority(a) - flowPriority(b))[0];
  return { total: required.length, remaining: pending.length, nextSlotId: next?.id ?? null, byTab };
}

/** Carried +N ammunition a weapon attack fires (engine `AttackSnapshot.ammunition`): the line's name and bonus. */
function ammunitionFor(c: MockCharacter, a: Attack): { name: string; bonus: number } | null {
  const weapon = c.items.find((i) => i.displayName === a.name && getItem(i.itemId)?.weapon);
  if (!weapon || !/ammunition/i.test(getItem(weapon.itemId)?.weapon?.properties ?? "")) return null;
  const ammo = c.items.find((i) => i.storage === 0 && getItem(i.itemId)?.ammoBonus);
  return ammo ? { name: ammo.displayName, bonus: getItem(ammo.itemId)?.ammoBonus ?? 0 } : null;
}

/** The modifier goes after the leading dice (`1d8 slashing` -> `1d8+3 slashing`), else at the end, as before. */
const LEADING_DICE = /^(\d+d\d+)([+-]\d+)?/;
function withModifier(damage: string, m: number): string {
  const mod = m ? signed(m) : "";
  return LEADING_DICE.test(damage) ? damage.replace(LEADING_DICE, (_all, dice: string) => `${dice}${mod}`) : damage.replace(/([+-]\d+)?$/, "") + mod;
}

/** The host's `toHit`: a numeric bonus with the D20 Test penalty, null for a save DC or free text. */
function toHitOf(c: MockCharacter, bonus: string): number | null {
  return /^\s*[+-]?\d+\s*$/.test(bonus) ? Number(bonus.trim().replace(/^\+/, "")) + d20Tests(c) : null;
}

function resolveAttacks(c: MockCharacter, mods: Record<string, number>, prof: number): Attack[] {
  return c.attacks.map(({ ammunition: _stale, ...a }) => {
    if (!a.auto || !a.ability) return { ...a, toHit: toHitOf(c, a.attackBonus) };
    const ammo = ammunitionFor(c, a);
    const extra = ammo?.bonus ?? 0;
    const m = (mods[a.ability] ?? 0) + extra;
    const attackBonus = signed(m + prof);
    // the host's per-ability preview of a weapon row (GitHub #27)
    const abilityLines = ABILITY_KEYS.map((k) => ({ ability: k, attackBonus: signed((mods[k] ?? 0) + extra + prof), damage: withModifier(a.damage, (mods[k] ?? 0) + extra) }));
    const resolved = { ...a, attackBonus, damage: withModifier(a.damage, m), toHit: toHitOf(c, attackBonus), abilityLines };
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
  const spellcasting = deriveSpellcasting(c, abilities);
  const companions = buildCompanions(c.companionName, c.companionPortrait, c.companions);
  const snapshot: CharacterSnapshot = {
    id: c.id, name: c.name, ruleset: c.ruleset, portrait: c.portrait, group: c.group, favorite: c.favorite, dirty: c.dirty, displayPath: c.path ?? "",
    level: deriveLevel(c), abilities, stats, slots, progress: progress(slots),
    spellcasting, inventory: inv, details: c.details,
    attacks: resolveAttacks(c, mods, profBonus(totalLevel(c))),
    companion: companions[0] ?? null, companions, options: c.options,
    availableOptions: OPTION_DEFS, sourceRestrictions: c.sourceRestrictions, notices: c.notices,
    play: buildPlay(c, spellcasting, mods.con),
  };
  return { character: c, snapshot };
}
