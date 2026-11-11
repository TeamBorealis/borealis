/**
 * Creature form values (WB-20, spec 4.11). Most fields hold the stat block's own shape of their `data` key (Rust
 * `borealis_content::MonsterData`, snake_case keys: `to_hit`, `per_day`); four fields are the form's own wrappers
 * (camelCase): the armor class and hit points with the note that stands in for them (IM12a), the senses with an unset
 * passive Perception, and the challenge with the XP and proficiency bonus left to the CR
 * (`borealis_authoring::form::{ArmorClassValue, HitPointsValue, SensesValue, ChallengeValue}`).
 */

export type CreatureAbility = "str" | "dex" | "con" | "int" | "wis" | "cha";

/** One armor class: `13 (natural armor)`, or `15 in humanoid form`. */
export interface HomebrewArmorClassEntry {
  value: number;
  source?: string;
  condition?: string;
}

/** `armorClass`: the entries, or the words for an armor class that is worked out (`note`; IM12a). */
export interface HomebrewArmorClassValue {
  entries: HomebrewArmorClassEntry[];
  note: string;
}

/** `hitPoints`: the hit dice give the average (`average: null` works it out); or the words for them (`note`). */
export interface HomebrewHitPointsValue {
  average: number | null;
  formula: string;
  note: string;
}

/** `speed` (feet). */
export interface HomebrewSpeeds {
  walk?: number;
  burrow?: number;
  climb?: number;
  fly?: number;
  swim?: number;
  hover?: boolean;
  note?: string;
}

/** `initiative` (2024): modifier and score (10 + modifier), or null. */
export interface HomebrewInitiative {
  modifier: number;
  score: number;
}

/** `abilities`: the six scores (1-30). */
export type HomebrewAbilities = Record<CreatureAbility, number>;

/** One damage line: the types it names, or its words when it says more ("from nonmagical attacks"). */
export interface HomebrewDamageClause {
  types: string[];
  text?: string;
}

/** `senses` (feet); `passive_perception: null` is 10 + the Perception bonus (or the Wisdom modifier). */
export interface HomebrewSensesValue {
  blindsight?: number;
  darkvision?: number;
  tremorsense?: number;
  truesight?: number;
  deafened_range?: number;
  blind_beyond?: boolean;
  passive_perception: number | null;
  notes?: string[];
}

/** `challenge`: the CR (`""` for none, then `note` stands in for it, IM12); `xp` and `proficiencyBonus` null follow the CR. */
export interface HomebrewChallengeValue {
  cr: string;
  note: string;
  xp: number | null;
  xpLair: number | null;
  proficiencyBonus: number | null;
}

/** How often an entry is used. */
export type HomebrewUsage =
  | { recharge: { min: number; max: number } }
  | { per_day: number }
  | { per_day_in_lair: { normal: number; lair: number } }
  | { on_rest: "short_or_long" | "long" };

export type HomebrewAttackKind =
  | "melee_weapon" | "ranged_weapon" | "melee_or_ranged_weapon" | "melee_spell" | "ranged_spell" | "melee" | "ranged" | "melee_or_ranged";

/** One damage part: `7 (2d6) fire`. */
export interface HomebrewDamagePart {
  average: number;
  dice?: string;
  type: string;
}

/** An entry's attack (its text stays what the stat block shows). */
export interface HomebrewAttack {
  kind: HomebrewAttackKind;
  to_hit: number;
  reach?: number;
  range?: number;
  long_range?: number;
  damage?: HomebrewDamagePart[];
}

/** An entry's saving throw. */
export interface HomebrewSaveEffect {
  ability: CreatureAbility;
  dc: number;
  damage?: HomebrewDamagePart[];
  half_on_success?: boolean;
}

export type HomebrewSpellFrequency = "at_will" | { per_day: { uses: number; each: boolean } };

/** A spell of a stat block's list: its name, the spell element when it is loaded, a note ("level 5 version"). */
export interface HomebrewStatSpell {
  name: string;
  id?: string;
  note?: string;
}

export interface HomebrewSpellList {
  frequency: HomebrewSpellFrequency;
  spells: HomebrewStatSpell[];
}

/** A Spellcasting entry's ability, DC, attack bonus and lists. */
export interface HomebrewStatSpellcasting {
  ability?: CreatureAbility;
  save_dc?: number;
  attack_bonus?: number;
  lists: HomebrewSpellList[];
}

/** A trait, action, bonus action, reaction or legendary action. */
export interface HomebrewCreatureEntry {
  name: string;
  text: string;
  usage?: HomebrewUsage;
  /** Legendary action cost. */
  cost?: number;
  /** "Humanoid Form Only". */
  condition?: string;
  attack?: HomebrewAttack;
  save?: HomebrewSaveEffect;
  spellcasting?: HomebrewStatSpellcasting;
}

/** `legendary`, or null. */
export interface HomebrewLegendary {
  per_round?: number;
  lair_uses?: number;
  preamble: string;
  actions: HomebrewCreatureEntry[];
}

export interface HomebrewLairOption {
  name?: string;
  text: string;
}

/** `lairActions` / `regionalEffects`, or null. */
export interface HomebrewLairSection {
  preamble?: string;
  options?: HomebrewLairOption[];
  closing?: string;
}
