import type { Attack, CharacterSnapshot, OptionDef, SelectionProgress, SelectionSlot, BuilderTab } from "../types";
import { getElement, getItem, titleCase } from "./content";
import { deriveAbilities } from "./derive-abilities";
import { deriveInventory } from "./derive-inventory";
import { deriveSpellcasting } from "./derive-spells";
import { deriveLevel, deriveStats, profBonus } from "./derive-stats";
import { buildPlay } from "./play";
import { type SlotDef, classIdOf, reprocess } from "./slots";
import { toWireSlotId } from "./slot-ids";
import { type MockCharacter, type RuleSettings, signed, totalLevel } from "./state";

const OPTION_DEFS: OptionDef[] = [
  { id: "multiclassing", name: "Multiclassing", description: "Allow taking levels in more than one class.", locked: false },
  { id: "feats", name: "Feats", description: "Offer feats at ability score improvement levels.", locked: false },
  { id: "averageHp", name: "Average hit points", description: "Use the fixed average for new levels instead of rolling.", locked: false },
];

const TABS: BuilderTab[] = ["race", "class", "background", "abilities", "feats", "languages", "proficiencies", "companion", "spells"];

export const toSlot = (d: SlotDef, c: MockCharacter): SelectionSlot => {
  const chosenId = c.choices[d.id];
  const el = chosenId ? getElement(chosenId) : undefined;
  const { candidates, dup: _dup, lockedTo: _lockedTo, ...rest } = d;
  const locked = d.locked || (d.required && candidates.length === 1 && d.elementType !== "class" && d.elementType !== "multiclass");
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

function resolveAttacks(c: MockCharacter, mods: Record<string, number>, prof: number): Attack[] {
  return c.attacks.map((a) => {
    if (!a.auto || !a.ability) return a;
    const m = mods[a.ability] ?? 0;
    return { ...a, attackBonus: signed(m + prof), damage: a.damage.replace(/([+-]\d+)?$/, "") + (m ? signed(m) : "") };
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
  const items = c.items.map((i) => ({ ...i, isWeapon: !!getItem(i.itemId)?.weapon }));
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
