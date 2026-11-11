import type {
  AbilityKey, Attack, CharacterSnapshot, HomebrewDraftView, HomebrewKindDescriptor, HomebrewLadderRow, HomebrewProblem, HomebrewTrialChange,
  HomebrewTrialSetup, HomebrewTrialView, InventoryItem, PlayFeature, Ruleset, SpellEntry, WeaponFacts,
} from "../types";
import type { MockElement } from "./data-types";
import { ABILITY_KEYS } from "../types";
import { getClass } from "./content";
import { apiError } from "./errors";
import { createCharacter } from "./library";
import { buildSlotDefs, type SlotDef } from "./slots";
import { findByWireId } from "./slot-ids";
import { buildSnapshot } from "./snapshot";
import { type MockCharacter, type RuleSettings, STANDARD_ARRAY } from "./state";

/** The draft a trial tries: its last view, its kind and the editions of its pack. */
export interface TrialDraft {
  view: HomebrewDraftView;
  kind: HomebrewKindDescriptor;
  rulesets: Ruleset[];
}

/** A trial's setup with every slot filled (spec 9: "other slots default to SRD picks of the edition"). */
interface ResolvedSetup {
  edition: Ruleset;
  level: number;
  abilityScores: number[];
  asSecondClass: boolean;
}

interface Trial {
  id: string;
  draftId: string;
  setup: ResolvedSetup;
  /** The choices by the mock's slot keys (`c0:class`, `race:0`, ...). */
  choices: Record<string, string>;
}

const MAX_LEVEL = 20;
/** The mock's SRD picks (the real backend takes the edition's SRD class, species and background). */
const DEFAULT_CHOICES: Record<string, string> = { "c0:class": "fighter", "race:0": "human", "bg:background": "soldier", "c0:subclass": "champion" };
/** The first feat slot of the mock's classes, as `engine_check::take_feat` takes it. */
const FIRST_FEAT_LEVEL = 4;
const ORIGIN_FEAT_LEVEL = 1;

const clampLevel = (n: number | undefined): number => Math.min(MAX_LEVEL, Math.max(1, Math.round(n ?? 1)));
const nameOf = (view: HomebrewDraftView): string => String(view.form.fields.name ?? "").trim() || "Unnamed";

/** The standard array assigned by the class's primary abilities, in STR..CHA order (spec 9). */
export function standardArrayFor(classId: string | null): number[] {
  const cls = getClass(classId);
  const order: AbilityKey[] = [...new Set<AbilityKey>([...(cls?.multiclassAbility ?? []), "con", ...(cls?.saves ?? []), "dex", "wis", "int", "cha", "str"])];
  return ABILITY_KEYS.map((k) => STANDARD_ARRAY[order.indexOf(k)] ?? 8);
}

/** The level the draft takes effect at in the trial, or 1 for drafts that apply from the start. */
function placementLevel(draft: TrialDraft, edition: Ruleset): number {
  if (draft.kind.elementTypes[0] !== "Feat") return 1;
  const origin = String(draft.view.form.fields.category ?? "").toLowerCase() === "origin";
  return edition === "dnd2024" && origin ? ORIGIN_FEAT_LEVEL : FIRST_FEAT_LEVEL;
}

/** Numbers a `stat` block of the draft changes in the mock: statistic name -> snapshot path. */
const STAT_PATHS: Record<string, string> = {
  speed: "stats.speed", "speed:misc": "stats.speed", "innate speed": "stats.speed", "innate speed:misc": "stats.speed",
  ac: "stats.armorClass", "ac:misc": "stats.armorClass", hp: "stats.hpMax", "hp:max": "stats.hpMax", initiative: "stats.initiative",
};
const ABILITY_STATS: Record<string, AbilityKey> = { strength: "str", dexterity: "dex", constitution: "con", intelligence: "int", wisdom: "wis", charisma: "cha" };

interface StatChange { path: string; by: number; from: number }

/** The draft's `stat` blocks, each from its own level and never before the draft is taken. */
function statChanges(draft: TrialDraft, edition: Ruleset): StatChange[] {
  const taken = placementLevel(draft, edition);
  return draft.view.form.blocks.flatMap((b) => {
    if (b.kind !== "stat") return [];
    const name = String(b.fields.name ?? "").toLowerCase();
    const by = Number(b.fields.value);
    const path = STAT_PATHS[name] ?? (ABILITY_STATS[name] ? `abilities.final.${ABILITY_STATS[name]}` : null);
    if (!path || !Number.isFinite(by) || by === 0) return [];
    return [{ path, by, from: Math.max(taken, Number(b.fields.level ?? 1) || 1) }];
  });
}

const ITEM_TYPES: readonly string[] = ["Item", "Weapon", "Armor", "Magic Item", "Item Pack", "Equipment Package"];
const refName = (v: unknown): string => (typeof v === "object" && v !== null && !Array.isArray(v) ? String((v as { name?: unknown }).name ?? "").trim() : "");
const refNames = (v: unknown): string[] => (Array.isArray(v) ? v.map(refName).filter(Boolean) : []);
const fieldText = (draft: TrialDraft, key: string): string => {
  const v = draft.view.form.fields[key];
  return typeof v === "string" ? v.trim() : "";
};

/** Does the item draft need attunement (its Attunement field set)? */
export function needsAttunement(fields: Readonly<Record<string, unknown>>): boolean {
  const v = fields.attunement;
  return v === true || (typeof v === "string" && v.trim() !== "" && v.trim().toLowerCase() !== "false");
}

/** A weapon draft's weapon facts, as the backend reads them from what the form writes. */
function weaponFacts(draft: TrialDraft): WeaponFacts | null {
  if (draft.kind.elementTypes[0] !== "Weapon") return null;
  const category = refName(draft.view.form.fields.weaponCategory);
  const [proficiency = "", kind = ""] = category.split(" ");
  return {
    damage: fieldText(draft, "damage"), damageType: refName(draft.view.form.fields.damageType).toLowerCase(),
    properties: refNames(draft.view.form.fields.properties), mastery: refNames(draft.view.form.fields.mastery).join(", ") || null,
    range: fieldText(draft, "range") || null, proficiency, kind,
  };
}

/** The weapon draft as a loaded weapon (the Weapon Mastery choice offers it, P2b), or null for any other draft. */
export function draftWeapon(draft: TrialDraft): MockElement | null {
  const weapon = weaponFacts(draft);
  if (!weapon) return null;
  const p = draft.view.preview;
  return {
    id: draft.view.draftId, name: nameOf(draft.view), type: "weapon", source: p?.source ?? "", sourceName: p?.sourceName ?? "",
    shortDescription: "", prerequisites: null, rulesets: draft.rulesets, weapon, description: p?.descriptionHtml ?? "", facts: p?.facts ?? [], grants: [],
  };
}

/** The draft's extra options in a choice of the trial character: a weapon draft in a Weapon Mastery choice. */
export function draftOptions(draft: TrialDraft, def: SlotDef): MockElement[] {
  const w = draftWeapon(draft);
  return w?.weapon?.mastery && def.title === "Weapon Mastery" ? [w] : [];
}

/** An item draft on the test character (spec 9): carried and, when it is worn or held, equipped; attuned only when it
 * needs attunement; a weapon is an attack. Never a feature. */
function withItem(s: CharacterSnapshot, draft: TrialDraft): { snapshot: CharacterSnapshot; changed: string[] } {
  const name = nameOf(draft.view);
  const fields = draft.view.form.fields;
  const weapon = weaponFacts(draft);
  const slot = fieldText(draft, "slot");
  const held = weapon !== null || slot !== "";
  const attuned = held && needsAttunement(fields);
  const strength = s.abilities.modifier.str;
  const dex = s.abilities.modifier.dex;
  const finesse = weapon?.properties.some((p) => /^finesse$/i.test(p)) ?? false;
  const ability: AbilityKey = weapon?.kind === "Ranged" || (finesse && dex > strength) ? "dex" : "str";
  const mod = s.abilities.modifier[ability];
  const item: InventoryItem = {
    id: `trial-${draft.view.draftId}`, itemId: draft.view.draftId, name, displayName: name, category: fieldText(draft, "category") || draft.kind.label,
    quantity: 1, weight: Number(fieldText(draft, "weight")) || 0, valueCp: Math.round((Number(fieldText(draft, "cost")) || 0) * 100),
    equipped: held ? "primary" : null, attuned, requiresAttunement: needsAttunement(fields), storage: 0, notes: "", listInInventory: true,
    isPack: false, packContents: [], isWeapon: weapon !== null, equippable: held, armor: null, overrideRequirements: false, withheldGrants: [],
    weapon: weapon ? { damage: weapon.damage, damageType: weapon.damageType, properties: weapon.properties.join(", "), ability, mastery: weapon.mastery, range: weapon.range } : null,
  };
  let next: CharacterSnapshot = { ...s, inventory: { ...s.inventory, items: [...s.inventory.items, item] } };
  const changed = [`inventory.items:${name}`];
  if (weapon) {
    const toHit = s.stats.proficiencyBonus + mod;
    const bonus = mod === 0 ? "" : mod > 0 ? ` + ${mod}` : ` - ${-mod}`;
    const attack: Attack = {
      id: item.id, name, range: weapon.range ?? "5 ft.", attackBonus: toHit >= 0 ? `+${toHit}` : String(toHit),
      damage: `${weapon.damage}${bonus} ${weapon.damageType}`.trim(), description: "", ability, auto: true, showInCombat: true, card: true, toHit,
    };
    next = { ...next, attacks: [...next.attacks, attack] };
    changed.push(`attacks:${name}`);
  }
  return { snapshot: next, changed };
}

/** The snapshot with the draft applied (the mock's stand-in for loading it into the engine), and what changed. */
function withDraft(s: CharacterSnapshot, draft: TrialDraft, level: number): { snapshot: CharacterSnapshot; changed: string[] } {
  const name = nameOf(draft.view);
  const type = draft.kind.elementTypes[0] ?? "";
  const at = placementLevel(draft, s.ruleset);
  const changed: string[] = [];
  let next = s;
  if (level >= at && type === "Spell") {
    const spell: SpellEntry = {
      id: draft.view.draftId, name, level: Number(draft.view.form.fields.level ?? 0) || 0, school: String(draft.view.form.fields.school ?? ""),
      prepared: true, alwaysPrepared: true, origin: "feat", slotId: null, canReplace: false, concentration: false, ritual: false, casting: null,
      sourceName: draft.view.preview?.sourceName ?? "", ready: true, grantedFrom: "Try it",
    };
    const block = { classSlotId: "caster-try-it", className: "Try it", ability: "int" as const, saveDc: 8 + s.stats.proficiencyBonus + s.abilities.modifier.int,
      attackBonus: s.stats.proficiencyBonus + s.abilities.modifier.int, attackRoll: s.stats.proficiencyBonus + s.abilities.modifier.int, model: "known" as const,
      cantripsMax: 0, cantripsKnown: 0, preparedMax: 0, preparedCount: 0, maxSpellLevel: 9, pact: null, spells: [spell], other: true };
    next = { ...next, spellcasting: { ...next.spellcasting, classes: [...next.spellcasting.classes, block] } };
    changed.push(`spellcasting.spells:${name}`);
  } else if (ITEM_TYPES.includes(type)) {
    const placed = withItem(next, draft);
    next = placed.snapshot;
    changed.push(...placed.changed);
  } else if (level >= at && type === "Language") {
    next = { ...next, stats: { ...next.stats, languages: [...next.stats.languages, name] } };
    changed.push(`stats.languages:${name}`);
  } else if (level >= at) {
    const feature: PlayFeature = { id: draft.view.draftId, name, ty: type || "Grant", source: draft.view.preview?.sourceName ?? "", level: at, description: draft.view.preview?.descriptionHtml ?? "", displayed: true };
    next = { ...next, play: { ...next.play, features: [...next.play.features, feature] }, stats: { ...next.stats, features: [...next.stats.features, { name, source: feature.source }] } };
    changed.push(`play.features:${name}`);
  }
  for (const c of statChanges(draft, s.ruleset).filter((x) => level >= x.from)) {
    next = applyStat(next, c);
    if (!changed.includes(c.path)) changed.push(c.path);
  }
  return { snapshot: next, changed };
}

function applyStat(s: CharacterSnapshot, c: StatChange): CharacterSnapshot {
  const [group, key, ability] = c.path.split(".");
  if (group === "abilities" && ability) {
    const k = ability as AbilityKey;
    const final = { ...s.abilities.final, [k]: s.abilities.final[k] + c.by };
    const modifier = { ...s.abilities.modifier, [k]: Math.floor((final[k] - 10) / 2) };
    return { ...s, abilities: { ...s.abilities, final, modifier } };
  }
  const stat = key as "speed" | "armorClass" | "hpMax" | "initiative";
  return { ...s, stats: { ...s.stats, [stat]: s.stats[stat] + c.by } };
}

/** In-memory Try it for the web mock (spec 9): a hidden mock character per trial, never listed or saved. */
export class MockTrials {
  private trials = new Map<string, Trial>();
  private ladders = new Map<string, { key: string; rows: HomebrewLadderRow[] }>();
  private next = 1;

  constructor(private deps: { rules: () => RuleSettings; draft: (draftId: string) => TrialDraft }) {}

  start(draftId: string, setup: HomebrewTrialSetup): HomebrewTrialView {
    const draft = this.deps.draft(draftId);
    const edition = setup.edition ?? draft.rulesets[draft.rulesets.length - 1] ?? "dnd2024";
    const trial: Trial = { id: `trial-${this.next++}`, draftId, setup: this.resolve(setup, edition, null), choices: { ...DEFAULT_CHOICES } };
    this.trials.set(trial.id, trial);
    return this.view(trial);
  }

  update(trialId: string, change: HomebrewTrialChange): HomebrewTrialView {
    const trial = this.trial(trialId);
    const editionChanged = !!change.setup?.edition && change.setup.edition !== trial.setup.edition;
    const merged: HomebrewTrialSetup = { ...change.setup, level: change.level ?? change.setup?.level ?? trial.setup.level };
    const setup = this.resolve(merged, change.setup?.edition ?? trial.setup.edition, trial.setup);
    // Classes never cross editions (R4): another edition starts from its own SRD picks.
    let choices = editionChanged ? { ...DEFAULT_CHOICES } : trial.choices;
    if (change.slotId) choices = this.pick({ ...trial, setup, choices }, change.slotId, change.elementId ?? null);
    const next = { ...trial, setup, choices };
    this.trials.set(trialId, next);
    return this.view(next);
  }

  end(trialId: string): void {
    this.trial(trialId);
    this.trials.delete(trialId);
    this.ladders.delete(trialId);
  }

  /** The hidden mock character behind a trial (for its choices' options). */
  character(trialId: string): MockCharacter {
    const t = this.trial(trialId);
    return this.build(t, t.setup.level);
  }

  /** The draft's own options in one of the trial character's choices (a weapon draft in Weapon Mastery). */
  extraOptions(trialId: string, def: SlotDef): MockElement[] {
    return draftOptions(this.deps.draft(this.trial(trialId).draftId), def);
  }

  private resolve(setup: HomebrewTrialSetup, edition: Ruleset, prev: ResolvedSetup | null): ResolvedSetup {
    const scores = setup.abilityScores ?? (prev && prev.edition === edition ? prev.abilityScores : standardArrayFor(DEFAULT_CHOICES["c0:class"]!));
    if (scores.length !== 6 || scores.some((n) => !Number.isInteger(n) || n < 1 || n > 30)) throw apiError("invalid_argument", "Ability scores are six whole numbers from 1 to 30.");
    return { edition, level: clampLevel(setup.level ?? prev?.level), abilityScores: scores, asSecondClass: setup.asSecondClass ?? prev?.asSecondClass ?? false };
  }

  private pick(t: Trial, slotId: string, elementId: string | null): Record<string, string> {
    const c = this.build(t, t.setup.level);
    const def = findByWireId(buildSlotDefs(c), slotId);
    if (!def) throw apiError("not_found", `Unknown slot ${slotId}`);
    if (elementId === null) {
      const { [def.id]: _cleared, ...rest } = t.choices;
      return rest;
    }
    if (def.locked) throw apiError("invalid_argument", "This selection is locked.");
    const offered = [...def.candidates, ...draftOptions(this.deps.draft(t.draftId), def)];
    if (!offered.some((e) => e.id === elementId)) throw apiError("requirement_failed", "That option is not available for this slot.");
    return { ...t.choices, [def.id]: elementId };
  }

  private build(t: Trial, level: number): MockCharacter {
    const base = createCharacter({ name: "Test character", ruleset: t.setup.edition, startingLevel: level, gender: "", abilityMethod: "manual", options: {} }, { playerName: "" });
    const [str = 8, dex = 8, con = 8, int = 8, wis = 8, cha = 8] = t.setup.abilityScores;
    return { ...base, id: t.id, base: { str, dex, con, int, wis, cha }, choices: { ...t.choices }, dirty: false };
  }

  private snapshotAt(t: Trial, level: number): { snapshot: CharacterSnapshot; changed: string[] } {
    const draft = this.deps.draft(t.draftId);
    return withDraft(buildSnapshot(this.build(t, level), this.deps.rules()).snapshot, draft, level);
  }

  private view(t: Trial): HomebrewTrialView {
    const draft = this.deps.draft(t.draftId);
    const { snapshot, changed } = this.snapshotAt(t, t.setup.level);
    // The ladder does not depend on the level: a level step reuses it.
    const key = JSON.stringify([t.setup.edition, t.setup.abilityScores, t.choices, draft.view.updatedAt, draft.view.form]);
    const kept = this.ladders.get(t.id);
    const ladder = kept?.key === key ? kept.rows : this.ladder(t, draft);
    this.ladders.set(t.id, { key, rows: ladder });
    return { trialId: t.id, snapshot, ladder, problems: ladder.flatMap((r) => r.problems), changed };
  }

  /** Levels 1-20 (spec 9): features gained, new choices, spell slots and the draft's problems at the level it applies. */
  private ladder(t: Trial, draft: TrialDraft): HomebrewLadderRow[] {
    const top = buildSnapshot(this.build(t, MAX_LEVEL), this.deps.rules()).snapshot;
    const at = placementLevel(draft, t.setup.edition);
    const problems: HomebrewProblem[] = draft.view.problems.filter((p) => p.severity !== "info");
    const blocks = statChanges(draft, t.setup.edition);
    return Array.from({ length: MAX_LEVEL }, (_, i) => {
      const level = i + 1;
      const s = buildSnapshot(this.build(t, level), this.deps.rules()).snapshot;
      const gained = top.slots.filter((x) => !x.locked && (x.requiredLevel ?? 1) === level);
      const features = [
        ...(level === 1 ? top.play.features.filter((f) => f.level === 1).map((f) => f.name) : []),
        ...gained.filter((x) => x.elementType === "archetype" || x.elementType === "asi").map((x) => x.title),
        ...(level === at ? [nameOf(draft.view)] : []),
        ...(blocks.some((b) => b.from === level && b.from > at) ? [`${nameOf(draft.view)} (more)`] : []),
      ];
      const slots = Array.from({ length: 9 }, (__, n) => s.spellcasting.slots.find((r) => r.level === n + 1)?.total ?? 0);
      return { level, features, choices: gained.map((x) => x.title), slots, problems: level === at ? problems : [] };
    });
  }

  private trial(trialId: string): Trial {
    const t = this.trials.get(trialId);
    if (!t) throw apiError("not_found", "The test character was closed. Open Try it again.");
    return t;
  }
}
