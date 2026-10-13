import type { AbilityKey, AbilityRecord, ArmorFacts, ElementRef, Ruleset, SpellFacts } from "./core";
import type { AttackMastery, PlayState } from "./play";

export type AbilityMethod = "point_buy" | "standard_array" | "roll_3d6" | "roll_4d6" | "manual";

export type BuilderTab =
  | "race"
  | "class"
  | "background"
  | "abilities"
  | "feats"
  | "languages"
  | "proficiencies"
  | "companion"
  | "spells";

/** Gender is a free string; the UI offers suggestions. */
export interface CharacterSummary {
  id: string;
  name: string;
  /** e.g. "Elf Wizard 3" */
  build: string;
  level: number;
  ruleset: Ruleset;
  /** data: URL or null (UI falls back to a generated monogram). */
  portrait: string | null;
  favorite: boolean;
  group: string;
  updatedAt: string;
  path: string | null;
  /** Content an Aurora import referenced that is still not installed (0 when none). */
  missingContent: number;
}

export interface CharacterOptions {
  multiclassing: boolean;
  feats: boolean;
  averageHp: boolean;
  /** House rule: allow point-buy to exceed the budget (the "custom" toggle). */
  pointBuyUnrestricted: boolean;
  /** Attunement limit (default 3). */
  attunementLimit: number;
  /** Custom option ids from installed content -> enabled. */
  custom: Record<string, boolean>;
}

export interface OptionDef {
  id: string;
  name: string;
  description: string;
  /** Forced by the ruleset (cannot be toggled). */
  locked: boolean;
}

export interface NewCharacterRequest {
  name: string;
  ruleset: Ruleset;
  startingLevel: number;
  gender: string;
  abilityMethod: AbilityMethod;
  options: Partial<CharacterOptions>;
  portrait?: string | null;
  group?: string;
}

/* ---------- Selection slots ---------- */

export interface SelectionSlot {
  /** Stable key = (owner element, rule index, ordinal). */
  id: string;
  tab: BuilderTab;
  /** Class-slot id this slot is grouped under (multiclass), else null. */
  groupId: string | null;
  groupLabel: string | null;
  /** Spell slots: `ClassSpellcasting.classSlotId` of the class the spell is learned for, else null (feat, species, other). */
  classSlotId: string | null;
  title: string;
  elementType: ElementRef["type"];
  /** 1-based pick number and total picks of the owning rule ("2 of 3"). */
  ordinal: number;
  of: number;
  required: boolean;
  /** Only one candidate / granted by rule: cannot be changed. */
  locked: boolean;
  chosen: ElementRef | null;
  requiredLevel: number | null;
  /** The filled spell may be replaced now (level-up swap rule; never 2014 cantrips). */
  allowReplace: boolean;
  hint: string | null;
  /** Level the slot was gained at: the rule level, or the class level it reopened at. */
  gainedOnLevel: number;
  /** Picks replaced in this slot, oldest first (Magic > Spellcasting "Replaced" column). */
  replaced: ReplacedPick[];
}

/** A pick replaced in a selection slot (spell retraining). */
export interface ReplacedPick {
  id: string;
  name: string;
  /** Class level at which it was replaced. */
  level: number;
}

export interface SelectionProgress {
  total: number;
  remaining: number;
  nextSlotId: string | null;
  byTab: Record<BuilderTab, number>;
}

/* ---------- Levels ---------- */

export interface ClassLevelInfo {
  slotId: string;
  classId: string | null;
  name: string | null;
  level: number;
  hitDie: number;
  isMain: boolean;
  subclass: ElementRef | null;
  /** Per-level hit point inputs; index 0 = level 1 (fixed max for main class). */
  hpRolls: number[];
  multiclassPrerequisite: string | null;
}

export interface LevelState {
  total: number;
  xp: number;
  /** XP needed for the next level, null at level 20. */
  nextLevelXp: number | null;
  currentLevelXp: number;
  /** XP crossed a threshold -> show a prompt, never auto-level. */
  levelUpAvailable: boolean;
  canLevelUp: boolean;
  canLevelDown: boolean;
  canMulticlass: boolean;
  multiclassBlockedReason: string | null;
  classes: ClassLevelInfo[];
}

/* ---------- Abilities ---------- */

export interface RollRecord {
  ability: AbilityKey;
  dice: number[];
  total: number;
}

export interface PointBuyState {
  budget: number;
  spent: number;
  remaining: number;
  costs: Record<number, number>;
  min: number;
  max: number;
  unrestricted: boolean;
}

export interface StandardArrayState {
  values: number[];
  assigned: AbilityRecord<number | null>;
  unassigned: number[];
}

export interface AbilityState {
  method: AbilityMethod;
  base: AbilityRecord<number>;
  /** Total bonus from race / background / ASIs / items. */
  bonus: AbilityRecord<number>;
  bonusSources: AbilityRecord<string[]>;
  final: AbilityRecord<number>;
  modifier: AbilityRecord<number>;
  save: AbilityRecord<{ bonus: number; proficient: boolean }>;
  cap: number;
  overCap: AbilityKey[];
  pointBuy: PointBuyState | null;
  standardArray: StandardArrayState | null;
  rolls: RollRecord[];
}

/* ---------- Derived stats ---------- */

export type Proficiency = "none" | "proficient" | "expertise";

export interface SkillEntry {
  key: string;
  name: string;
  ability: AbilityKey;
  bonus: number;
  proficiency: Proficiency;
}

export interface CharacterStats {
  proficiencyBonus: number;
  armorClass: number;
  initiative: number;
  speed: number;
  hpMax: number;
  passivePerception: number;
  skills: SkillEntry[];
  languages: string[];
  proficiencies: { label: string; values: string[] }[];
  /** Selected features / traits, for the overview panel. */
  features: { name: string; source: string }[];
}

/* ---------- Spellcasting ---------- */

export type SpellModel = "known" | "prepared" | "spellbook";

export interface SpellEntry {
  id: string;
  name: string;
  level: number;
  school: string;
  prepared: boolean;
  alwaysPrepared: boolean;
  origin: "class" | "race" | "feat" | "item";
  /** Selection slot holding this spell (for replace), if chosen via a slot. */
  slotId: string | null;
  canReplace: boolean;
  concentration: boolean;
  ritual: boolean;
  /** Casting time, range, duration and V/S/M; null when the spell has no content element. */
  casting: SpellFacts | null;
  /** Source book the spell comes from (empty when unknown). */
  sourceName: string;
}

export interface ClassSpellcasting {
  classSlotId: string;
  className: string;
  ability: AbilityKey;
  saveDc: number;
  attackBonus: number;
  model: SpellModel;
  cantripsMax: number;
  cantripsKnown: number;
  preparedMax: number;
  preparedCount: number;
  /** Highest spell level the class can cast at this level. */
  maxSpellLevel: number;
  /** Pact Magic slots: a separate pool recovered on a short or long rest (never in `SpellcastingState.slots`). */
  pact: PactSlotRow | null;
  spells: SpellEntry[];
}

/** Pact Magic slots: all of one level. */
export interface PactSlotRow {
  level: number;
  total: number;
}

export interface SpellSlotRow {
  level: number;
  total: number;
}

export interface SpellcastingState {
  classes: ClassSpellcasting[];
  slots: SpellSlotRow[];
  multiclassSlots: SpellSlotRow[] | null;
}

/* ---------- Inventory ---------- */

export type EquipSlot = "worn" | "primary" | "secondary" | "two-handed";
export type EquipRequest = EquipSlot | "auto" | "versatile";

export interface Coins {
  cp: number;
  sp: number;
  ep: number;
  gp: number;
  pp: number;
}
export type CoinKey = keyof Coins;

export interface InventoryItem {
  /** Instance id (not the content id). */
  id: string;
  itemId: string;
  name: string;
  displayName: string;
  category: string;
  quantity: number;
  weight: number;
  valueCp: number;
  equipped: EquipSlot | null;
  attuned: boolean;
  requiresAttunement: boolean;
  /** 0 = carried, 1/2 = stored in the named storage containers. */
  storage: 0 | 1 | 2;
  notes: string;
  listInInventory: boolean;
  /** Item-card flag ("show card"); absent on older hosts, treated as off. */
  showCard?: boolean;
  isPack: boolean;
  packContents: string[];
  isWeapon: boolean;
  /** Weapon facts for the 'add to attacks' flow; null for non-weapons. */
  weapon: {
    damage: string;
    damageType: string;
    properties: string;
    ability: AbilityKey;
    /** Weapon Mastery property (2024 content). */
    mastery: string | null;
    /** Normal/long range in feet, e.g. "80/320". */
    range: string | null;
  } | null;
  /** Armor columns; null for non-armor. */
  armor: ArmorFacts | null;
}

export interface InventoryState {
  items: InventoryItem[];
  coins: Coins;
  totalGp: number;
  carriedWeight: number;
  capacity: number;
  attunement: { used: number; max: number };
  storageNames: [string, string];
  /** E.g. "Attunement limit reached: equipped unattuned". Transient. */
  warnings: string[];
}

/* ---------- Manage ---------- */

export interface CharacterDetails {
  gender: string;
  playerName: string;
  alignment: string;
  deity: string;
  age: string;
  height: string;
  weight: string;
  eyes: string;
  skin: string;
  hair: string;
  features: string;
  backstory: string;
  personalityTraits: string;
  ideal: string;
  bond: string;
  flaw: string;
  trinket: string;
  notesLeft: string;
  notesRight: string;
  organization: string;
  allies: string;
  combatNotes: string;
  sheetAcNote: string;
  /** Additional treasure (free text). */
  treasure: string;
  /** Quest items (free text). */
  questItems: string;
}

export interface Attack {
  id: string;
  name: string;
  range: string;
  /** Free text when manual, computed (e.g. "+5") when auto. */
  attackBonus: string;
  damage: string;
  description: string;
  /** Linked ability for auto attacks. */
  ability: AbilityKey | null;
  auto: boolean;
  showInCombat: boolean;
  card: boolean;
  /** Computed (auto) attacks only: 2024 weapon mastery property; `mastered` once chosen. */
  mastery?: AttackMastery | null;
  /** Lowest d20 roll that is a critical hit (20 unless a feature lowers it). */
  critRange?: number;
  /** Damage dice never roll below this (Great Weapon Fighting). */
  dieFloor?: number | null;
  /** Off-hand damage when it differs (Light weapons). */
  offhandDamage?: string | null;
}

export interface CompanionState {
  creature: ElementRef | null;
  name: string;
  portrait: string | null;
  armorClass: number;
  hp: number;
  speed: number;
  abilities: AbilityRecord<number>;
}

/* ---------- Snapshot ---------- */

export interface CharacterSnapshot {
  id: string;
  name: string;
  ruleset: Ruleset;
  portrait: string | null;
  group: string;
  favorite: boolean;
  /** Unsaved changes. */
  dirty: boolean;
  /** Display-only path of the character file (empty when unsaved). Never sent back to a command. */
  displayPath: string;
  level: LevelState;
  abilities: AbilityState;
  stats: CharacterStats;
  slots: SelectionSlot[];
  progress: SelectionProgress;
  spellcasting: SpellcastingState;
  inventory: InventoryState;
  details: CharacterDetails;
  attacks: Attack[];
  companion: CompanionState | null;
  options: CharacterOptions;
  availableOptions: OptionDef[];
  /** Enabled source ids; null = no restriction. */
  sourceRestrictions: string[] | null;
  /** Informational diagnostics (e.g. legacy divergences on import). */
  notices: string[];
  /** Resources, toggles, defences, senses, notes, Wild Shape, packages, compatibility, features. */
  play: PlayState;
}

/** One foreign-edition element registered on a character. */
export interface CompatEntry {
  element: ElementRef;
  from: Ruleset;
  /** The adapter changed the element. */
  converted: boolean;
  notes: string[];
}

/** `get_compatibility_report`: foreign-edition content in use and whether strict mode is on. */
export interface CompatibilityReport {
  /** New picks show same-edition content only. */
  strict: boolean;
  entries: CompatEntry[];
}

export interface RulesetProfile {
  id: Ruleset;
  label: string;
  srdSource: string;
  terminology: { species: string; armorTraining: string; inspiration: string };
  abilityMethods: AbilityMethod[];
  abilityCap: number;
  pointBuyBudget: number;
  weaponMastery: boolean;
  originFeats: boolean;
  backgroundAsi: boolean;
}

export interface MissingContentItem {
  id: string;
  /** Element type ("Class Feature", "Spell", ...); empty when the file did not say. */
  type: string;
  name: string;
  /** Where the file referenced it: option, selection, inventory, dependency, ... */
  context: string;
  /** Source book guessed from the id ("WOTC PHB"); may be empty. */
  source: string;
}

export interface UnmappedSlotItem {
  owner: string;
  type: string;
  name: string;
  chosen: string;
  reason: string;
}

export interface ImportReport {
  summary: CharacterSummary;
  /** LegacyDivergence diagnostics. */
  divergences: string[];
  warnings: string[];
  /** Referenced content that is not installed. */
  missingContent: MissingContentItem[];
  /** Selections that could not be placed; kept on the character for a re-check. */
  unmappedSlots: UnmappedSlotItem[];
  invalidSelections: string[];
  selectionsImported: number;
}

export interface RecheckEntry {
  characterId: string;
  name: string;
  resolved: number;
  stillMissing: MissingContentItem[];
  stillUnmapped: UnmappedSlotItem[];
}

export interface RecheckReport {
  /** Characters that had something outstanding and were looked at. */
  checked: number;
  /** Characters that picked up content. */
  updated: RecheckEntry[];
  /** Characters that still have missing content. */
  outstanding: RecheckEntry[];
}
