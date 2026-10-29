import type { ConversionPreview, PlayResource, PlayState, PlayToggle, PlayWildShape, SpellcastingState } from "../types";
import { getClass, getElement, getItem } from "./content";
import { apiError } from "./errors";
import { chargeResources } from "./rolled-charges";
import { classIdOf } from "./slots";
import { type MockTracking, afterRest, trackerPlay } from "./spell-tracker";
import { type MockCharacter, totalLevel } from "./state";

/** Mutable play data of a mock character (kept optional on `MockCharacter`). */
export interface MockPlay {
  used: Record<string, number>;
  toggles: string[];
  exhaustion: number;
  inspiration: boolean;
  masteries: string[];
  forms: string[];
  packages: Record<string, string>;
  /** Spell slots, concentration and custom trackers (GitHub #2); absent = nothing tracked. */
  tracking?: MockTracking;
}

export const emptyPlay = (): MockPlay => ({ used: {}, toggles: [], exhaustion: 0, inspiration: false, masteries: [], forms: [], packages: {} });
const playOf = (c: MockCharacter): MockPlay => c.play ?? emptyPlay();
const MAX_EXHAUSTION = 6;
/** Weapon Mastery picks the classes of a 2024 character allow. */
const masteryCapacity = (c: MockCharacter): number =>
  c.ruleset === "dnd2024" ? c.classes.reduce((n, cs) => n + (getClass(classIdOf(c, cs.slotId))?.masteries ?? 0), 0) : 0;

const resourcesFor = (c: MockCharacter): PlayResource[] => {
  const level = Math.max(1, totalLevel(c));
  const base = { source: "Mock class", recharge: ["short_rest", "long_rest"], shortRestRegain: 1, longRestRegainAll: true };
  return [{ ...base, id: "rage", name: "Rage", max: 2 + Math.floor(level / 4), current: 0 }, ...chargeResources(c)];
};

const TOGGLES: Omit<PlayToggle, "on" | "active">[] = [
  { id: "rage", name: "Rage", activation: "Bonus Action", duration: "1 minute", blocks: ["spellcasting"], effects: ["Resistance to Bludgeoning, Piercing and Slashing damage"], source: "Mock class" },
];

/** Item states (Boots of Speed): equipped and attuned items with a `speedToggle`, as the engine's `speed_multiply` toggle. */
function itemToggles(c: MockCharacter): (Omit<PlayToggle, "on" | "active"> & { factor: number })[] {
  return c.items.flatMap((i) => {
    const factor = getItem(i.itemId)?.speedToggle;
    if (!factor || !i.equipped || !i.attuned) return [];
    const name = getElement(i.itemId)?.name ?? i.name;
    return [{ id: `${i.itemId}_SPEED`, name, activation: "bonus action", duration: "", blocks: [], effects: [`Walk speed x${factor}`], source: name, factor }];
  });
}
const togglesOf = (c: MockCharacter) => [...TOGGLES.map((t) => ({ ...t, factor: 1 })), ...itemToggles(c)];

/** The mock's `CharacterSnapshot.play` (`sc` = the snapshot's spellcasting, for the spell tracker). */
export function buildPlay(c: MockCharacter, sc: SpellcastingState): PlayState {
  const p = playOf(c);
  const resources = resourcesFor(c).map((r) => ({ ...r, current: Math.max(0, r.max - (p.used[r.id] ?? 0)) }));
  const toggles = togglesOf(c).map(({ factor: _factor, ...t }) => ({ ...t, on: p.toggles.includes(t.id), active: p.toggles.includes(t.id) }));
  const raging = toggles.some((t) => t.id === "rage" && t.active);
  const walk = togglesOf(c).filter((t) => p.toggles.includes(t.id)).reduce((w, t) => w * t.factor, 30);
  return {
    speeds: { walk: walk - 5 * p.exhaustion, fly: 0, climb: 0, swim: 0, burrow: 0 },
    resources, resourceCosts: [], toggles,
    exhaustion: { linear: c.ruleset === "dnd2024", level: p.exhaustion, maxLevels: MAX_EXHAUSTION, d20Penalty: -2 * p.exhaustion, speedPenalty: -5 * p.exhaustion, dead: p.exhaustion >= MAX_EXHAUSTION },
    inspiration: p.inspiration, inspirationSources: [], martialArts: null,
    // As the host: `weapon` is the element id, `name` the weapon's name.
    masteries: p.masteries.map((w) => ({ weapon: w, name: getElement(w)?.name ?? w, property: "Graze", text: "Deal ability-modifier damage on a miss." })),
    defenses: {
      resistances: ["bludgeoning", "piercing", "slashing"].map((name) => ({ name, source: "Rage", active: raging, whileToggle: "rage" })),
      immunities: [], vulnerabilities: [], conditionImmunities: [],
    },
    senses: [{ name: "darkvision", range: 60, source: "Mock species" }], advantages: [{ roll: "saving throw", text: "Advantage against being charmed.", source: "Mock species", active: true }],
    initiativeAdvantage: false, notes: [{ kind: "note", text: "Mock note.", source: "Mock class" }], carry: null,
    wildShape: buildWildShape(c),
    equipmentPackages: [{
      package: "Mock starting equipment", name: "Mock starting equipment", owner: "Mock class", chosen: p.packages["Mock starting equipment"] ?? null,
      options: [
        { id: "A", items: [{ id: null, name: "Longsword", qty: 1, note: null }, { id: null, name: "Shield", qty: 1, note: null }], coins: zeroCoins },
        { id: "B", items: [{ id: null, name: "Greataxe", qty: 1, note: null }], coins: { ...zeroCoins, gp: 10 } },
      ],
    }],
    compatibility: { strict: c.ruleset === "dnd2014", entries: [] },
    features: [{ id: "mock-feature", name: "Mock Feature", ty: "class_feature", source: "Mock class", level: 1, description: "<p>A <strong>mock</strong> feature description.</p>", displayed: true }],
    freeCasts: [], saveDcs: [],
    ...trackerPlay(p.tracking, sc),
  };
}

const zeroCoins = { cp: 0, sp: 0, ep: 0, gp: 0, pp: 0 };
const withPlay = (c: MockCharacter, fn: (p: MockPlay) => MockPlay): MockCharacter => ({ ...c, play: fn(playOf(c)) });

export function spend(c: MockCharacter, id: string, uses: number): MockCharacter {
  const r = resourcesFor(c).find((x) => x.id === id);
  if (!r) throw apiError("not_found", "Unknown resource.");
  return withPlay(c, (p) => {
    const used = (p.used[id] ?? 0) + uses;
    if (used > r.max) throw apiError("requirement_failed", `${r.name} has ${r.max - (p.used[id] ?? 0)} of ${r.max} uses left`);
    return { ...p, used: { ...p.used, [id]: used } };
  });
}

export const restore = (c: MockCharacter, id: string, uses: number): MockCharacter =>
  withPlay(c, (p) => ({ ...p, used: { ...p.used, [id]: Math.max(0, (p.used[id] ?? 0) - uses) } }));

export const shortRest = (c: MockCharacter): MockCharacter =>
  withPlay(c, (p) => ({ ...p, toggles: [], used: Object.fromEntries(Object.entries(p.used).map(([k, v]) => [k, Math.max(0, v - 1)])), tracking: afterRest(p.tracking, "short") }));

export const longRest = (c: MockCharacter): MockCharacter =>
  withPlay(c, (p) => ({ ...p, toggles: [], used: {}, exhaustion: c.ruleset === "dnd2024" ? Math.max(0, p.exhaustion - 1) : p.exhaustion, tracking: afterRest(p.tracking, "long") }));

/** Applies a spell tracker update to the mock's play state. */
export const withTracking = (c: MockCharacter, fn: (t: MockTracking | undefined) => MockTracking): MockCharacter =>
  withPlay(c, (p) => ({ ...p, tracking: fn(p.tracking) }));

export function setToggle(c: MockCharacter, id: string, on: boolean): MockCharacter {
  if (!togglesOf(c).some((t) => t.id === id)) throw apiError("not_found", "Unknown toggle.");
  return withPlay(c, (p) => ({ ...p, toggles: on ? [...new Set([...p.toggles, id])] : p.toggles.filter((t) => t !== id) }));
}

export function setExhaustion(c: MockCharacter, level: number): MockCharacter {
  if (level > MAX_EXHAUSTION) throw apiError("requirement_failed", `Exhaustion goes up to ${MAX_EXHAUSTION}.`);
  return withPlay(c, (p) => ({ ...p, exhaustion: level }));
}

export const setInspiration = (c: MockCharacter, has: boolean): MockCharacter => withPlay(c, (p) => ({ ...p, inspiration: has }));

export function chooseMasteries(c: MockCharacter, weapons: string[]): MockCharacter {
  if (new Set(weapons).size !== weapons.length) throw apiError("invalid_argument", "Each weapon can be chosen once.");
  const capacity = masteryCapacity(c);
  if (weapons.length > capacity) throw apiError("requirement_failed", `Only ${capacity} weapons can be mastered at this level.`);
  return withPlay(c, (p) => ({ ...p, masteries: weapons }));
}

/** Beasts a Druid may learn (id, name, CR, AC, HP, speed). */
const BEASTS: PlayWildShape["options"] = [
  { id: "wolf", name: "Wolf", cr: "1/4", ac: 13, hp: 11, speed: "40 ft." },
  { id: "brown-bear", name: "Brown Bear", cr: "1", ac: 11, hp: 34, speed: "40 ft., climb 30 ft." },
  { id: "riding-horse", name: "Riding Horse", cr: "1/4", ac: 10, hp: 13, speed: "60 ft." },
  { id: "giant-eagle", name: "Giant Eagle", cr: "1", ac: 13, hp: 26, speed: "10 ft., fly 80 ft." },
];
const WILD_SHAPE_LEVEL = 2;
const druidLevel = (c: MockCharacter): number =>
  c.classes.filter((cs) => classIdOf(c, cs.slotId) === "druid").reduce((n, cs) => n + cs.hpRolls.length, 0);

/** Wild Shape limits by Druid level (2024 table, simplified). */
function wildShapeLimits(level: number): { maxForms: number; maxCr: string; flyAllowed: boolean } {
  if (level >= 8) return { maxForms: 8, maxCr: "1", flyAllowed: true };
  if (level >= 4) return { maxForms: 6, maxCr: "1/2", flyAllowed: false };
  return { maxForms: 4, maxCr: "1/4", flyAllowed: false };
}
const crValue = (cr: string): number => (cr.includes("/") ? Number(cr.split("/")[0]) / Number(cr.split("/")[1]) : Number(cr));

function buildWildShape(c: MockCharacter): PlayWildShape | null {
  const level = druidLevel(c);
  if (level < WILD_SHAPE_LEVEL) return null;
  const limits = wildShapeLimits(level);
  const fits = (f: PlayWildShape["options"][number]) => crValue(f.cr) <= crValue(limits.maxCr) && (limits.flyAllowed || !f.speed.includes("fly"));
  const learned = playOf(c).forms;
  return {
    source: "Wild Shape", druidLevel: level, changeOn: "long_rest", ...limits, durationHours: Math.floor(level / 2), tempHp: level,
    known: BEASTS.filter((f) => learned.includes(f.id) && fits(f)), options: BEASTS.filter(fits),
    invalid: learned.filter((id) => !BEASTS.some((f) => f.id === id && fits(f))),
  };
}

export function setForms(c: MockCharacter, forms: string[]): MockCharacter {
  const ws = buildWildShape(c);
  if (!ws) throw apiError("requirement_failed", "The character has no Wild Shape.");
  if (forms.length > ws.maxForms) throw apiError("requirement_failed", `Only ${ws.maxForms} forms can be known at this level.`);
  const bad = forms.find((id) => !ws.options.some((f) => f.id === id));
  if (bad) throw apiError("requirement_failed", `${bad} does not fit the Wild Shape limits.`);
  return withPlay(c, (p) => ({ ...p, forms }));
}

export function choosePackage(c: MockCharacter, pkg: string, option: string): MockCharacter {
  if (playOf(c).packages[pkg]) throw apiError("requirement_failed", `${pkg} was already taken.`);
  return withPlay(c, (p) => ({ ...p, packages: { ...p.packages, [pkg]: option } }));
}

/** Mock "Convert to 2024": everything is kept; the copy is renamed. */
export function previewConversion(c: MockCharacter): ConversionPreview {
  if (c.ruleset !== "dnd2014") throw apiError("requirement_failed", `Only 2014 characters can be converted to 2024 (this one is ${c.ruleset}).`);
  const stats = { level: totalLevel(c), hitPoints: 0, armorClass: 10, initiative: 0, proficiencyBonus: 2, speed: 30, abilityScores: [10, 10, 10, 10, 10, 10] as ConversionPreview["before"]["abilityScores"], spellSaveDc: null };
  const items: ConversionPreview["items"] = Object.values(c.choices).map((id) => ({ category: "choice", label: id, outcome: "kept", fromId: id, toId: id, reason: null, what: null, crossEdition: true, kind: null }));
  return { characterId: c.id, name: `${c.name} (2024)`, items, counts: { kept: items.length, mapped: 0, dropped: 0, needsChoice: 0 }, before: stats, after: stats };
}
