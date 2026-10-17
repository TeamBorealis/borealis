import type { AbilityKey, ElementSummary, Ruleset } from "../types";

export interface MockElement extends ElementSummary {
  description: string;
  facts: { label: string; value: string }[];
  grants: string[];
}

export interface RaceDef {
  id: string;
  speed: number;
  bonus2014: Partial<Record<AbilityKey, number>>;
  subraces: string[];
}

export interface ClassDef {
  id: string;
  hitDie: number;
  saves: AbilityKey[];
  skillCount: number;
  skills: string[];
  fightingStyle: boolean;
  subclassLevel: Record<Ruleset, number>;
  subclasses: string[];
  caster: null | { ability: AbilityKey; model: "spellbook" | "prepared" };
  masteries: number;
  multiclassAbility: AbilityKey[];
}

export interface BackgroundDef {
  id: string;
  skills: string[];
  languages: number;
  asi: AbilityKey[];
  originFeat: string;
}

export interface ItemDef {
  id: string;
  category: string;
  costCp: number;
  weight: number;
  attune?: boolean;
  /** Attunement restriction: printed text and the classes that qualify (the engine's `attunement` requirements). */
  attuneBy?: { text: string; classes: string[] };
  /** A switchable state while the item is equipped and attuned that multiplies the walking speed (Boots of Speed). */
  speedToggle?: number;
  armor?: { base: number; dexCap: number | null; shield?: boolean; strength?: number; stealth?: boolean };
  weapon?: { damage: string; mastery?: string; properties: string; range?: string; martial?: boolean };
  bonusAc?: number;
  pack?: string[];
  description: string;
}

export interface SpellDef {
  id: string;
  level: number;
  school: string;
  classes: string[];
  concentration: boolean;
  ritual: boolean;
  description: string;
  time: string;
  range: string;
  /** Components, any of V, S, M. Default "VS". */
  components?: string;
  material?: string;
  duration?: string;
}
