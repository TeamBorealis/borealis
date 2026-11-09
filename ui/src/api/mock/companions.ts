import type { AbilityRecord, CompanionFeature, CompanionPatch, CompanionState, ElementType } from "../types";
import type { MockElement } from "./data-types";
import { apiError } from "./errors";
import { type MockCharacter, mod } from "./state";

/** Mirrors the backend limits (`borealis_character::companions`). */
export const MAX_ADDED_COMPANIONS = 12;
const MAX_TRAITS = 30;
const MAX_NAME = 60;
const MAX_TEXT = 2000;
const MAX_NOTES = 4000;
const MAX_BONUS = 99;
export const GRANTED_ID = "granted";

/** The user's notes, traits and bonuses on one companion. */
export interface MockCompanionEdits { notes: string; traits: CompanionFeature[]; armorClassBonus: number; hitPointsBonus: number; speedBonus: number }
/** A companion added by hand. */
export interface MockAddedCompanion extends MockCompanionEdits { id: string; creatureId: string | null; name: string; portrait: string | null }
/** What the mock keeps about a character's companions (the backend's top-level `companions` plus the granted creature). */
export interface MockCompanions { grantedCreature: string | null; granted: MockCompanionEdits; added: MockAddedCompanion[] }

export const NO_EDITS: MockCompanionEdits = { notes: "", traits: [], armorClassBonus: 0, hitPointsBonus: 0, speedBonus: 0 };
export const NO_COMPANIONS: MockCompanions = { grantedCreature: null, granted: NO_EDITS, added: [] };

interface Creature {
  id: string; name: string; build: string; ac: number; hp: number; speed: number; fly: number; prof: number;
  abilities: AbilityRecord<number>; skills: [name: string, ability: string, bonus: number][]; senses: string; traits: CompanionFeature[]; actions: CompanionFeature[];
}

/** Ability name and abbreviation of the six saving throws, STR to CHA. */
const SAVES: [key: keyof AbilityRecord<number>, name: string, abbr: string][] = [
  ["str", "Strength", "STR"], ["dex", "Dexterity", "DEX"], ["con", "Constitution", "CON"],
  ["int", "Intelligence", "INT"], ["wis", "Wisdom", "WIS"], ["cha", "Charisma", "CHA"],
];

/** Three SRD 5.1 beasts (CC-BY-4.0) the mock offers as companion creatures. */
const CREATURES: Creature[] = [
  {
    id: "ID_SRD51_MONSTER_WOLF", name: "Wolf", build: "Medium beast, unaligned", ac: 13, hp: 11, speed: 40, fly: 0, prof: 2,
    abilities: { str: 12, dex: 15, con: 12, int: 3, wis: 12, cha: 6 }, skills: [["Perception", "WIS", 3], ["Stealth", "DEX", 4]], senses: "",
    traits: [{ name: "Keen Hearing and Smell", text: "The wolf has advantage on Wisdom (Perception) checks that rely on hearing or smell." },
      { name: "Pack Tactics", text: "The wolf has advantage on an attack roll against a creature if at least one of the wolf's allies is within 5 feet of the creature and the ally isn't incapacitated." }],
    actions: [{ name: "Bite", text: "Melee Weapon Attack: +4 to hit, reach 5 ft., one target. Hit: 7 (2d4 + 2) piercing damage. If the target is a creature, it must succeed on a DC 11 Strength saving throw or be knocked prone." }],
  },
  {
    id: "ID_SRD51_MONSTER_OWL", name: "Owl", build: "Tiny beast, unaligned", ac: 11, hp: 1, speed: 5, fly: 60, prof: 2,
    abilities: { str: 3, dex: 13, con: 8, int: 2, wis: 12, cha: 7 }, skills: [["Perception", "WIS", 3], ["Stealth", "DEX", 3]], senses: "Darkvision 120 ft.",
    traits: [{ name: "Flyby", text: "The owl doesn't provoke opportunity attacks when it flies out of an enemy's reach." },
      { name: "Keen Hearing and Sight", text: "The owl has advantage on Wisdom (Perception) checks that rely on hearing or sight." }],
    actions: [{ name: "Talons", text: "Melee Weapon Attack: +3 to hit, reach 5 ft., one target. Hit: 1 slashing damage." }],
  },
  {
    id: "ID_SRD51_MONSTER_MASTIFF", name: "Mastiff", build: "Medium beast, unaligned", ac: 12, hp: 5, speed: 40, fly: 0, prof: 2,
    abilities: { str: 13, dex: 14, con: 12, int: 3, wis: 12, cha: 7 }, skills: [["Perception", "WIS", 3]], senses: "",
    traits: [{ name: "Keen Hearing and Smell", text: "The mastiff has advantage on Wisdom (Perception) checks that rely on hearing or smell." }],
    actions: [{ name: "Bite", text: "Melee Weapon Attack: +3 to hit, reach 5 ft., one target. Hit: 4 (1d6 + 1) piercing damage. If the target is a creature, it must succeed on a DC 11 Strength saving throw or be knocked prone." }],
  },
];

const creature = (id: string | null) => CREATURES.find((c) => c.id === id) ?? null;

/** The creatures as compendium elements (type `companion`) so the picker's search finds them. */
export function companionElements(make: (id: string, type: ElementType, name: string, short: string, extra: Partial<MockElement>) => MockElement): MockElement[] {
  return CREATURES.map((c) => make(c.id, "companion", c.name, `${c.build}.`, {
    facts: [{ label: "Type", value: "Companion" }, { label: "Armor Class", value: String(c.ac) }, { label: "Hit Points", value: String(c.hp) }],
  }));
}

const withBonus = (n: number, bonus: number) => n + bonus;

function stateOf(id: string, granted: boolean, creatureId: string | null, name: string, portrait: string | null, e: MockCompanionEdits): CompanionState {
  const c = creature(creatureId);
  const abilities = c?.abilities ?? { str: 10, dex: 10, con: 10, int: 10, wis: 10, cha: 10 };
  const ref = creatureId ? { id: creatureId, name: c?.name ?? creatureId, type: "companion" as const, source: c ? "SRD 5.1" : "" } : null;
  const skills = (c?.skills ?? []).map(([n, ability, bonus]) => ({ name: n, ability, bonus }));
  const perception = skills.find((s) => s.name === "Perception")?.bonus ?? mod(abilities.wis);
  return {
    id, granted, creature: ref, name: name || ref?.name || "", portrait,
    armorClass: withBonus(c?.ac ?? 0, e.armorClassBonus), hp: withBonus(c?.hp ?? 0, e.hitPointsBonus), armorClassNote: null, hpNote: null,
    challenge: "—", speed: withBonus(c?.speed ?? 0, e.speedBonus), abilities, proficiency: c?.prof ?? 0, initiative: mod(abilities.dex), build: c?.build ?? "",
    flySpeed: c?.fly ?? 0, climbSpeed: 0, swimSpeed: 0, burrowSpeed: 0, skills, passivePerception: 10 + perception,
    saves: SAVES.map(([k, n, ability]) => ({ name: n, ability, bonus: mod(abilities[k]) })),
    senses: c?.senses ?? "", languages: "—", defenses: [],
    traits: [...(c?.traits ?? []), ...e.traits], actions: c?.actions ?? [], reactions: [],
    notes: e.notes, customTraits: e.traits, armorClassBonus: e.armorClassBonus, hitPointsBonus: e.hitPointsBonus, speedBonus: e.speedBonus,
  };
}

const isEmptyEdits = (e: MockCompanionEdits) => !e.notes && e.traits.length === 0 && !e.armorClassBonus && !e.hitPointsBonus && !e.speedBonus;

/** Every companion like the backend lists them: the granted one first (when there is one), then the added ones. */
export function buildCompanions(name: string, portrait: string | null, k: MockCompanions = NO_COMPANIONS): CompanionState[] {
  const hasGranted = !!k.grantedCreature || !!name || !!portrait || !isEmptyEdits(k.granted);
  const granted = hasGranted ? [stateOf(GRANTED_ID, true, k.grantedCreature, name, portrait, k.granted)] : [];
  return [...granted, ...k.added.map((a) => stateOf(a.id, false, a.creatureId, a.name, a.portrait, a))];
}

/* ---------- validation (the backend's messages and codes) ---------- */

const invalid = (message: string) => apiError("invalid_argument", message);

function checkedName(name: string): string {
  const n = name.trim();
  // eslint-disable-next-line no-control-regex
  if ([...n].length > MAX_NAME || /[\u0000-\u001f\u007f]/.test(n)) throw invalid(`A companion name is at most ${MAX_NAME} characters, without control characters.`);
  return n;
}

function checkedEdits(e: MockCompanionEdits): MockCompanionEdits {
  if ([...e.notes].length > MAX_NOTES) throw invalid(`Notes is longer than ${MAX_NOTES} characters.`);
  if (e.traits.length > MAX_TRAITS) throw invalid(`A companion can have at most ${MAX_TRAITS} traits.`);
  for (const t of e.traits) {
    const n = [...t.name.trim()].length;
    if (n === 0 || n > MAX_NAME) throw invalid(`A trait name must be 1 to ${MAX_NAME} characters, without control characters.`);
    if ([...t.text].length > MAX_TEXT) throw invalid(`A trait's text is longer than ${MAX_TEXT} characters.`);
  }
  for (const b of [e.armorClassBonus, e.hitPointsBonus, e.speedBonus]) {
    if (!Number.isInteger(b) || Math.abs(b) > MAX_BONUS) throw invalid(`Bonuses must be whole numbers between -${MAX_BONUS} and ${MAX_BONUS}.`);
  }
  return { ...e, notes: e.notes.trimEnd(), traits: e.traits.map((t) => ({ name: t.name.trim(), text: t.text.trimEnd() })) };
}

function checkedCreature(id: string | null | undefined): string | null {
  if (!id?.trim()) return null;
  if (!creature(id.trim())) throw apiError("not_found", "That creature is not in the installed content.");
  return id.trim();
}

function editsFrom(old: MockCompanionEdits, p: CompanionPatch): MockCompanionEdits {
  return checkedEdits({
    notes: p.notes ?? old.notes, traits: p.traits ?? old.traits, armorClassBonus: p.armorClassBonus ?? old.armorClassBonus,
    hitPointsBonus: p.hitPointsBonus ?? old.hitPointsBonus, speedBonus: p.speedBonus ?? old.speedBonus,
  });
}

/* ---------- operations (pure: they return new values) ---------- */

export function addCompanion(k: MockCompanions, creatureId: string | null | undefined, name: string | undefined): MockCompanions {
  if (k.added.length >= MAX_ADDED_COMPANIONS) throw apiError("requirement_failed", `A character can have at most ${MAX_ADDED_COMPANIONS} added companions.`);
  const top = Math.max(0, ...k.added.map((a) => Number(a.id.replace("companion-", "")) || 0));
  const added: MockAddedCompanion = { ...NO_EDITS, id: `companion-${top + 1}`, creatureId: checkedCreature(creatureId), name: checkedName(name ?? ""), portrait: null };
  return { ...k, added: [...k.added, added] };
}

export function removeCompanion(k: MockCompanions, id: string): MockCompanions {
  if (id === GRANTED_ID) throw apiError("requirement_failed", "This companion comes from a feature: change it on the builder's Companion tab.");
  if (!k.added.some((a) => a.id === id)) throw apiError("not_found", `No companion has the id "${id}".`);
  return { ...k, added: k.added.filter((a) => a.id !== id) };
}

export function updateGranted(k: MockCompanions, p: CompanionPatch): MockCompanions {
  if (p.creatureId !== undefined) throw invalid(`Unknown companion field "creatureId".`);
  return { ...k, granted: editsFrom(k.granted, p) };
}

export function updateAdded(k: MockCompanions, id: string, p: CompanionPatch): MockCompanions {
  const old = k.added.find((a) => a.id === id);
  if (!old) throw apiError("not_found", `No companion has the id "${id}".`);
  const next: MockAddedCompanion = {
    ...editsFrom(old, p), id, name: p.name === undefined ? old.name : checkedName(p.name),
    portrait: p.portrait === undefined ? old.portrait : p.portrait, creatureId: p.creatureId === undefined ? old.creatureId : checkedCreature(p.creatureId),
  };
  return { ...k, added: k.added.map((a) => (a.id === id ? next : a)) };
}

/** `updateCompanion`: the granted companion keeps its name and picture on the character, as before. */
export function updateCompanionOf(c: MockCharacter, companionId: string | undefined, p: CompanionPatch): MockCharacter {
  const k = c.companions ?? NO_COMPANIONS;
  if (companionId && companionId !== GRANTED_ID) return { ...c, companions: updateAdded(k, companionId, p) };
  const { name, portrait, ...edits } = p;
  return {
    ...c, companionName: name === undefined ? c.companionName : name.trim(),
    companionPortrait: portrait === undefined ? c.companionPortrait : portrait, companions: updateGranted(k, edits),
  };
}

/** `?mockCompanions=1`: Brannoc has a granted Mastiff and two added companions (GitHub #13 screenshots). */
export const SAMPLE_COMPANIONS: MockCompanions = {
  grantedCreature: "ID_SRD51_MONSTER_MASTIFF",
  granted: { ...NO_EDITS, notes: "Follows Brannoc from the old garrison." },
  added: [
    { ...NO_EDITS, id: "companion-1", creatureId: "ID_SRD51_MONSTER_OWL", name: "Hoot", portrait: null, speedBonus: 10,
      traits: [{ name: "Night Watch", text: "Hoot wakes the camp when anything larger than a fox comes near." }] },
    { ...NO_EDITS, id: "companion-2", creatureId: "ID_SRD51_MONSTER_WOLF", name: "Ash", portrait: null, armorClassBonus: 1, hitPointsBonus: 4,
      traits: [{ name: "Scarred Hide", text: "Thick old scars harden its hide." }] },
  ],
};
