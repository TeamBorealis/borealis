/** 2024 play state (`CharacterSnapshot.play`) and "Convert to 2024". */
import type { CharacterSnapshot } from "./character";

export interface Speeds { walk: number; fly: number; climb: number; swim: number; burrow: number }

/** A limited-use pool (Rage, Focus Points, Channel Divinity, ...). */
export interface PlayResource {
  id: string;
  name: string;
  max: number;
  current: number;
  source: string;
  /** short_rest, long_rest, initiative, ... */
  recharge: string[];
  /** Uses a Short Rest gives back; null = none. */
  shortRestRegain: number | null;
  longRestRegainAll: boolean;
  otherRegain?: string[];
}

export interface PlayResourceCost { resource: string; option: string; cost: number; [k: string]: unknown }

/** A switchable state such as Rage. `on` = the user switched it on, `active` = its effects apply. */
export interface PlayToggle {
  id: string;
  name: string;
  on: boolean;
  active: boolean;
  /** Why an enabled toggle has no effect (worn armor). */
  blockedBy?: string;
  activation: string;
  duration: string;
  blocks: string[];
  effects: string[];
  source: string;
}

export interface PlayExhaustion {
  linear: boolean;
  level: number;
  maxLevels: number;
  /** Zero or negative. */
  d20Penalty: number;
  speedPenalty: number;
  dead: boolean;
}

export interface PlayDefenseEntry {
  /** Damage type or condition, lower case. */
  name: string;
  source: string;
  /** In effect now (false while its toggle is off). */
  active: boolean;
  whileToggle?: string;
}

export interface PlayDefenses {
  resistances: PlayDefenseEntry[];
  immunities: PlayDefenseEntry[];
  vulnerabilities: PlayDefenseEntry[];
  conditionImmunities: PlayDefenseEntry[];
}

export interface PlaySense { name: string; range: number; source: string; note?: string }
export interface PlayAdvantage { roll: string; text: string; source: string; active: boolean; whileToggle?: string }
export interface PlayNote { kind: string; text: string; source: string; [k: string]: unknown }
export interface PlayMastery { weapon: string; name: string; property: string; text: string }
export interface PlayInspirationSource { source: string; trigger: string }
export interface PlayMartialArts { die: string; active: boolean; bonusUnarmedStrike: boolean }

export interface WildShapeForm {
  id: string;
  name: string;
  cr: string;
  ac: number;
  hp: number;
  speed: string;
  [k: string]: unknown;
}
export interface PlayWildShape {
  /** Feature that grants Wild Shape. */
  source: string;
  druidLevel: number;
  /** When known forms may be changed (`long_rest`). */
  changeOn: string;
  maxForms: number;
  maxCr: string;
  flyAllowed: boolean;
  durationHours: number;
  tempHp: number;
  known: WildShapeForm[];
  options: WildShapeForm[];
  invalid: string[];
}

export interface PackageItem { id: string | null; name: string; qty: number; note: string | null }
export interface PackageCoins { cp: number; sp: number; ep: number; gp: number; pp: number }
export interface PackageOption { id: string; items: PackageItem[]; coins: PackageCoins }
export interface PlayEquipmentPackage {
  /** Package element id (pass to `chooseEquipmentPackage`). */
  package: string;
  name: string;
  /** Class or background that offers it. */
  owner: string;
  /** Option id already taken (A/B/C); null until chosen. */
  chosen: string | null;
  options: PackageOption[];
}

/** A feature, trait or feat with its sanitized HTML description. */
export interface PlayFeature {
  id: string;
  name: string;
  ty: string;
  source: string;
  level: number;
  grantedBy?: string;
  description: string;
  sheetSummary?: string;
  displayed: boolean;
}

export interface PlayCompatEntry { [k: string]: unknown }
export interface PlayCompatibility { strict: boolean; entries: PlayCompatEntry[] }

/** Used and maximum slots of one spell level, or of the Pact Magic pool (GitHub #2). */
export interface PlaySlotUse {
  level: number;
  max: number;
  used: number;
}

/** A spell the character can concentrate on (known or prepared, with concentration). */
export interface PlayConcentrationSpell {
  id: string;
  name: string;
  /** 0 = cantrip. */
  level: number;
}

/** When a custom tracker gets its uses back. */
export type PlayTrackerReset = "short_rest" | "long_rest" | "never";

/** A counter the user adds on Play (Drakkenheim contamination, Splicer points). */
export interface PlayTracker {
  id: string;
  name: string;
  /** 1..999. */
  max: number;
  /** Marked pips, 0..max (gained, when it counts up). */
  used: number;
  /** A file from a newer version may hold another value; it then never resets here. */
  reset: PlayTrackerReset;
  /** Shown counting up: used/max with the used pips filled (Drakkenheim contamination). */
  countsUp: boolean;
}

export interface PlayState {
  speeds: Speeds;
  resources: PlayResource[];
  resourceCosts: PlayResourceCost[];
  toggles: PlayToggle[];
  exhaustion: PlayExhaustion;
  /** Heroic Inspiration (2024). */
  inspiration: boolean;
  inspirationSources: PlayInspirationSource[];
  martialArts: PlayMartialArts | null;
  masteries: PlayMastery[];
  defenses: PlayDefenses;
  senses: PlaySense[];
  advantages: PlayAdvantage[];
  initiativeAdvantage: boolean;
  notes: PlayNote[];
  /** Carrying capacity with size steps; null unless a feature grants steps. */
  carry: Record<string, unknown> | null;
  wildShape: PlayWildShape | null;
  equipmentPackages: PlayEquipmentPackage[];
  compatibility: PlayCompatibility;
  features: PlayFeature[];
  freeCasts: Record<string, unknown>[];
  saveDcs: Record<string, unknown>[];
  /** Spell slots per level that has slots (the multiclass table when multiclassed); back on a long rest. */
  spellSlots: PlaySlotUse[];
  /** Pact Magic slots, back on a short or long rest; null without Pact Magic. */
  pactSlots: PlaySlotUse | null;
  /** The spell being concentrated on; ends on a long rest. */
  concentration: PlayConcentrationSpell | null;
  /** Concentration spells the character knows or has prepared, by level and name. */
  concentrationSpells: PlayConcentrationSpell[];
  trackers: PlayTracker[];
}

export interface AttackMastery { name: string; text: string; mastered: boolean }

export type ConversionOutcome = "kept" | "mapped" | "dropped" | "needs_choice";
export type ConversionCategory =
  | "species" | "background" | "class" | "subclass" | "feat" | "spell" | "item" | "language" | "ability_score" | "choice";

export interface ConversionItem {
  category: ConversionCategory;
  label: string;
  outcome: ConversionOutcome;
  fromId: string | null;
  toId: string | null;
  /** Why it was dropped. */
  reason: string | null;
  /** What the user must still choose. */
  what: string | null;
  /** Kept as a 2014 element inside the 2024 character. */
  crossEdition: boolean;
  kind: string | null;
}

export interface ConversionStats {
  level: number;
  hitPoints: number;
  armorClass: number;
  initiative: number;
  proficiencyBonus: number;
  speed: number;
  abilityScores: [number, number, number, number, number, number];
  spellSaveDc: number | null;
}

export interface ConversionPreview {
  characterId: string;
  /** Name of the copy ("... (2024)"). */
  name: string;
  items: ConversionItem[];
  counts: { kept: number; mapped: number; dropped: number; needsChoice: number };
  before: ConversionStats;
  after: ConversionStats;
}

export interface ConversionApplied { newCharacterId: string; snapshot: CharacterSnapshot }
