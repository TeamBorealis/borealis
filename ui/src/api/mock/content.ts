import type { AbilityKey, ElementType, Ruleset } from "../types";
import { armorFacts, spellFacts, weaponFacts } from "./facts";
import type { BackgroundDef, ClassDef, ItemDef, MockElement, RaceDef, SpellDef } from "./data-types";
import { monsterElements } from "./monsters";

/** Original, SRD-generic sample content for the mock backend. */
export const SOURCE_2014 = "sample-srd-51";
export const SOURCE_2024 = "sample-srd-52";
/** B-55: an imported (not bundled) 2024 source, so a picker row can carry an imported version. */
export const SOURCE_EXPANSION = "sample-expansion";
/** Sources of the bundled packs (the SRD stand-ins); everything else counts as imported. */
export const BUNDLED_SOURCES: readonly string[] = [SOURCE_2014, SOURCE_2024];
const SRC_NAME: Record<string, string> = {
  [SOURCE_2014]: "Sample Reference 5.1",
  [SOURCE_2024]: "Sample Reference 5.2",
  [SOURCE_EXPANSION]: "Sample Expansion",
  "sample-homebrew": "Sample Homebrew",
};
const BOTH: Ruleset[] = ["dnd2014", "dnd2024"];

function el(
  id: string,
  type: ElementType,
  name: string,
  short: string,
  extra: Partial<MockElement> = {},
): MockElement {
  const source = extra.source ?? SOURCE_2014;
  return {
    id,
    type,
    name,
    source,
    sourceName: SRC_NAME[source] ?? source,
    shortDescription: short,
    prerequisites: null,
    rulesets: BOTH,
    description: `<p>${short}</p>`,
    facts: [],
    grants: [],
    ...extra,
  };
}

export const SKILLS: { key: string; name: string; ability: AbilityKey }[] = [
  { key: "acrobatics", name: "Acrobatics", ability: "dex" },
  { key: "animal-handling", name: "Animal Handling", ability: "wis" },
  { key: "arcana", name: "Arcana", ability: "int" },
  { key: "athletics", name: "Athletics", ability: "str" },
  { key: "deception", name: "Deception", ability: "cha" },
  { key: "history", name: "History", ability: "int" },
  { key: "insight", name: "Insight", ability: "wis" },
  { key: "intimidation", name: "Intimidation", ability: "cha" },
  { key: "investigation", name: "Investigation", ability: "int" },
  { key: "medicine", name: "Medicine", ability: "wis" },
  { key: "nature", name: "Nature", ability: "int" },
  { key: "perception", name: "Perception", ability: "wis" },
  { key: "performance", name: "Performance", ability: "cha" },
  { key: "persuasion", name: "Persuasion", ability: "cha" },
  { key: "religion", name: "Religion", ability: "int" },
  { key: "sleight", name: "Sleight of Hand", ability: "dex" },
  { key: "stealth", name: "Stealth", ability: "dex" },
  { key: "survival", name: "Survival", ability: "wis" },
];

export const LANGUAGES = ["Common", "Dwarvish", "Elvish", "Giant", "Gnomish", "Halfling", "Orc", "Draconic", "Celestial", "Abyssal"];

export const ABILITY_NAMES: Record<AbilityKey, string> = {
  str: "Strength", dex: "Dexterity", con: "Constitution", int: "Intelligence", wis: "Wisdom", cha: "Charisma",
};

export const RACES: RaceDef[] = [
  { id: "human", speed: 30, bonus2014: { str: 1, dex: 1, con: 1, int: 1, wis: 1, cha: 1 }, subraces: [] },
  { id: "elf", speed: 30, bonus2014: { dex: 2 }, subraces: ["high-elf", "wood-elf"] },
  { id: "dwarf", speed: 25, bonus2014: { con: 2 }, subraces: ["hill-dwarf", "mountain-dwarf"] },
  { id: "halfling", speed: 25, bonus2014: { dex: 2 }, subraces: ["lightfoot"] },
  // B-55: one name in three versions (2014 bundled, 2024 bundled, 2024 imported). Its ancestry pick exists in both
  // editions, and the two Red Ancestry elements share a name, so a version switch can keep or drop it.
  { id: "dragonborn", speed: 30, bonus2014: { str: 2, cha: 1 }, subraces: ["ancestry-red", "ancestry-blue"], subTitle: "Draconic Ancestry" },
  { id: "dragonborn-2024", speed: 30, bonus2014: {}, subraces: ["ancestry-red-2024", "ancestry-gold"], subTitle: "Draconic Ancestry" },
  { id: "dragonborn-expansion", speed: 30, bonus2014: {}, subraces: ["ancestry-red", "ancestry-blue", "ancestry-gold"], subTitle: "Draconic Ancestry" },
];

/** B-55: races written for one edition or carried by another source (same name, separate elements). */
const RACE_EXTRA: Record<string, Partial<MockElement>> = {
  dragonborn: { rulesets: ["dnd2014"] },
  "dragonborn-2024": { name: "Dragonborn", rulesets: ["dnd2024"], source: SOURCE_2024 },
  "dragonborn-expansion": { name: "Dragonborn", rulesets: ["dnd2024"], source: SOURCE_EXPANSION },
  "ancestry-red-2024": { name: "Red Ancestry", rulesets: ["dnd2024"], source: SOURCE_2024 },
  "ancestry-gold": { source: SOURCE_2024 },
};

export const SUBRACE_BONUS: Record<string, Partial<Record<AbilityKey, number>>> = {
  "high-elf": { int: 1 }, "wood-elf": { wis: 1 }, "hill-dwarf": { wis: 1 }, "mountain-dwarf": { str: 2 }, lightfoot: { cha: 1 },
};

export const CLASSES: ClassDef[] = [
  { id: "fighter", hitDie: 10, saves: ["str", "con"], skillCount: 2, skills: ["acrobatics", "athletics", "history", "insight", "intimidation", "perception", "survival"],
    fightingStyle: true, subclassLevel: { dnd2014: 3, dnd2024: 3 }, subclasses: ["champion", "tactician"], caster: null, masteries: 3, multiclassAbility: ["str", "dex"] },
  { id: "wizard", hitDie: 6, saves: ["int", "wis"], skillCount: 2, skills: ["arcana", "history", "insight", "investigation", "medicine", "religion"],
    fightingStyle: false, subclassLevel: { dnd2014: 2, dnd2024: 3 }, subclasses: ["evoker", "diviner"], caster: { ability: "int", model: "spellbook" }, masteries: 0, multiclassAbility: ["int"] },
  { id: "cleric", hitDie: 8, saves: ["wis", "cha"], skillCount: 2, skills: ["history", "insight", "medicine", "persuasion", "religion"],
    fightingStyle: false, subclassLevel: { dnd2014: 1, dnd2024: 3 }, subclasses: ["life-domain", "light-domain"], caster: { ability: "wis", model: "prepared" }, masteries: 0, multiclassAbility: ["wis"] },
  { id: "rogue", hitDie: 8, saves: ["dex", "int"], skillCount: 4, skills: ["acrobatics", "athletics", "deception", "insight", "intimidation", "investigation", "perception", "performance", "persuasion", "sleight", "stealth"],
    fightingStyle: false, subclassLevel: { dnd2014: 3, dnd2024: 3 }, subclasses: ["thief", "shadow-blade"], caster: null, masteries: 2, multiclassAbility: ["dex"] },
  { id: "druid", hitDie: 8, saves: ["int", "wis"], skillCount: 2, skills: ["arcana", "insight", "medicine", "perception", "survival"],
    fightingStyle: false, subclassLevel: { dnd2014: 2, dnd2024: 3 }, subclasses: ["circle-of-the-land", "circle-of-the-moon"], caster: { ability: "wis", model: "prepared" }, masteries: 0, multiclassAbility: ["wis"] },
];

export const BACKGROUNDS: BackgroundDef[] = [
  { id: "acolyte", skills: ["insight", "religion"], languages: 2, asi: ["int", "wis", "cha"], originFeat: "magic-initiate" },
  { id: "soldier", skills: ["athletics", "intimidation"], languages: 0, asi: ["str", "dex", "con"], originFeat: "savage-attacker" },
  { id: "sage", skills: ["arcana", "history"], languages: 2, asi: ["con", "int", "wis"], originFeat: "skilled" },
];

const raceText: Record<string, [string, string]> = {
  human: ["Versatile and ambitious, found in every corner of the world.", "Speed 30 ft. Adaptable in every pursuit."],
  elf: ["Graceful folk with keen senses and long memories.", "Darkvision, keen senses, trance instead of sleep."],
  dwarf: ["Stout, sturdy folk with deep roots in stone and craft.", "Darkvision, poison resilience, stonecunning."],
  halfling: ["Small, nimble travellers with remarkable luck.", "Lucky, brave, halfling nimbleness."],
  "high-elf": ["An elf schooled in minor magic.", "Learn one wizard cantrip."],
  "wood-elf": ["An elf attuned to forests, fleet of foot.", "Speed 35 ft."],
  "hill-dwarf": ["A dwarf of rolling highlands, wise and tough.", "Extra hit point each level."],
  "mountain-dwarf": ["A dwarf of the high peaks, strong and martial.", "Armor training."],
  lightfoot: ["A halfling who moves unseen among larger folk.", "Naturally stealthy."],
  dragonborn: ["Proud folk with the blood of dragons, written for the older rules.", "Breath weapon once per rest; damage resistance."],
  "dragonborn-2024": ["Proud folk with the blood of dragons, written for the newer rules.", "Breath weapon in place of an attack; darkvision."],
  "dragonborn-expansion": ["Proud folk with the blood of dragons, as an expansion retells them.", "Breath weapon, resistance and a gliding descent."],
  "ancestry-red": ["Your ancestor breathed fire.", "Fire breath and fire resistance."],
  "ancestry-blue": ["Your ancestor breathed lightning.", "Lightning breath and lightning resistance."],
  "ancestry-red-2024": ["Your ancestor breathed fire.", "Fire breath, fire resistance, and flight later on."],
  "ancestry-gold": ["Your ancestor breathed fire and gleamed like coin.", "Fire breath and fire resistance."],
};
const title = (s: string) => s.split("-").map((w) => w[0]!.toUpperCase() + w.slice(1)).join(" ");

const classText: Record<string, string> = {
  fighter: "A master of martial combat, skilled with a variety of weapons and armor.",
  wizard: "A scholarly magic-user who studies the arcane through a spellbook.",
  cleric: "A priestly champion who wields divine magic in service of a higher power.",
  rogue: "A scoundrel who uses stealth and cunning to overcome obstacles.",
  druid: "A priest of the Old Faith who wields nature magic and assumes animal forms.",
};
const subText: Record<string, string> = {
  champion: "Improved critical hits and athletic prowess.",
  tactician: "Maneuvers that turn tactics into advantage.",
  evoker: "Sculpt evocation magic to spare allies.",
  diviner: "Foresee outcomes; replace rolls with portents.",
  "life-domain": "Heal more, and heal better.",
  "light-domain": "Radiant fire and protective flares.",
  thief: "Fast hands and quick climbs.",
  "shadow-blade": "Strike from darkness with lethal precision.",
  "circle-of-the-land": "Draw magic from the land you protect.",
  "circle-of-the-moon": "Fight in beast form with a stronger Wild Shape.",
};

export const FEATS: MockElement[] = [
  el("alert", "feat", "Alert", "Always ready for danger; bonus to initiative.", { rulesets: BOTH, tags: ["origin"] }),
  el("magic-initiate", "feat", "Magic Initiate", "Learn two cantrips and one 1st-level spell from a class list.", { rulesets: ["dnd2024"], source: SOURCE_2024, tags: ["origin"] }),
  el("savage-attacker", "feat", "Savage Attacker", "Reroll weapon damage once per turn.", { rulesets: ["dnd2024"], source: SOURCE_2024, tags: ["origin"] }),
  el("skilled", "feat", "Skilled", "Gain proficiency in three skills or tools.", { rulesets: ["dnd2024"], source: SOURCE_2024, tags: ["origin"] }),
  el("tough", "feat", "Tough", "Hit point maximum increases by twice your level.", { tags: ["general"] }),
  el("lucky", "feat", "Lucky", "Spend luck points to reroll d20s.", { tags: ["general"] }),
  el("sentinel", "feat", "Sentinel", "Lock down enemies with opportunity attacks.", { prerequisites: "Strength or Dexterity 13", tags: ["general"] }),
  // An epic boon: offered by no level-4 select, and added through Equipment > Additional Feat it is withheld below 19 (U12).
  el("boon-of-fate", "feat", "Boon of Fate", "Roll 2d4 to change a nearby D20 Test once per rest.", { rulesets: ["dnd2024"], source: SOURCE_2024, prerequisites: "Level 19+", tags: ["epic boon"] }),
];

export const FIGHTING_STYLES = ["archery", "defense", "dueling", "great-weapon-fighting"].map((id) =>
  el(id, "fighting-style", title(id), `The ${title(id).toLowerCase()} fighting style.`),
);

export const LISTS: Record<string, MockElement[]> = {
  trait: ["I am always polite and respectful.", "I face problems head-on.", "I am full of questions."].map((t, i) => el(`trait-${i}`, "list", t, t)),
  ideal: ["Tradition", "Freedom", "Knowledge"].map((t, i) => el(`ideal-${i}`, "list", t, t)),
  bond: ["I owe my life to a mentor.", "My home needs protecting.", "I seek a lost relic."].map((t, i) => el(`bond-${i}`, "list", t, t)),
  flaw: ["I judge others too quickly.", "I cannot resist a dare.", "I trust the wrong people."].map((t, i) => el(`flaw-${i}`, "list", t, t)),
};

export const SPELLS: SpellDef[] = [
  { id: "fire-bolt", level: 0, components: "VS", school: "Evocation", classes: ["wizard"], concentration: false, ritual: false, time: "1 action", range: "120 ft", description: "Hurl a mote of fire at a creature or object." },
  { id: "mage-hand", level: 0, school: "Conjuration", classes: ["wizard"], concentration: false, ritual: false, time: "1 action", range: "30 ft", description: "A spectral hand manipulates small objects." },
  { id: "light", components: "VM", material: "a firefly or phosphorescent moss", duration: "1 hour", level: 0, school: "Evocation", classes: ["wizard", "cleric"], concentration: false, ritual: false, time: "1 action", range: "Touch", description: "An object sheds bright light." },
  { id: "prestidigitation", level: 0, school: "Transmutation", classes: ["wizard"], concentration: false, ritual: false, time: "1 action", range: "10 ft", description: "Minor magical tricks." },
  { id: "sacred-flame", level: 0, school: "Evocation", classes: ["cleric"], concentration: false, ritual: false, time: "1 action", range: "60 ft", description: "Radiance descends on a creature." },
  { id: "guidance", level: 0, school: "Divination", classes: ["cleric"], concentration: true, ritual: false, time: "1 action", range: "Touch", description: "A creature adds 1d4 to one check." },
  { id: "magic-missile", level: 1, school: "Evocation", classes: ["wizard"], concentration: false, ritual: false, time: "1 action", range: "120 ft", description: "Three darts of force that never miss." },
  { id: "shield", duration: "1 round", level: 1, school: "Abjuration", classes: ["wizard"], concentration: false, ritual: false, time: "1 reaction", range: "Self", description: "+5 AC until your next turn." },
  { id: "sleep", components: "VSM", material: "a pinch of fine sand, rose petals, or a cricket", duration: "1 minute", level: 1, school: "Enchantment", classes: ["wizard"], concentration: false, ritual: false, time: "1 action", range: "90 ft", description: "Creatures fall unconscious." },
  { id: "mage-armor", components: "VSM", material: "a piece of cured leather", duration: "8 hours", level: 1, school: "Abjuration", classes: ["wizard"], concentration: false, ritual: false, time: "1 action", range: "Touch", description: "Base AC becomes 13 + Dex." },
  { id: "burning-hands", level: 1, school: "Evocation", classes: ["wizard"], concentration: false, ritual: false, time: "1 action", range: "Self (15-ft cone)", description: "A thin sheet of flames shoots from your fingertips." },
  { id: "charm-person", level: 1, school: "Enchantment", classes: ["wizard"], concentration: false, ritual: false, time: "1 action", range: "30 ft", description: "A humanoid regards you as a friendly acquaintance." },
  { id: "feather-fall", level: 1, school: "Transmutation", classes: ["wizard"], concentration: false, ritual: false, time: "1 reaction", range: "60 ft", description: "Creatures falling slow to a gentle descent." },
  { id: "thunderwave", level: 1, school: "Evocation", classes: ["wizard"], concentration: false, ritual: false, time: "1 action", range: "Self (15-ft cube)", description: "A wave of thunderous force sweeps outward." },
  { id: "identify", components: "VSM", material: "a pearl worth at least 100 gp and an owl feather", level: 1, school: "Divination", classes: ["wizard"], concentration: false, ritual: true, time: "1 minute", range: "Touch", description: "Learn an item's properties." },
  { id: "detect-magic", level: 1, school: "Divination", classes: ["wizard", "cleric"], concentration: true, ritual: true, time: "1 action", range: "Self", description: "Sense magic within 30 ft." },
  { id: "cure-wounds", level: 1, school: "Abjuration", classes: ["cleric"], concentration: false, ritual: false, time: "1 action", range: "Touch", description: "Restore hit points by touch." },
  { id: "bless", level: 1, school: "Enchantment", classes: ["cleric"], concentration: true, ritual: false, time: "1 action", range: "30 ft", description: "Allies add 1d4 to attacks and saves." },
  { id: "healing-word", level: 1, school: "Abjuration", classes: ["cleric"], concentration: false, ritual: false, time: "1 bonus action", range: "60 ft", description: "Restore hit points at range." },
  { id: "misty-step", level: 2, school: "Conjuration", classes: ["wizard"], concentration: false, ritual: false, time: "1 bonus action", range: "Self", description: "Teleport up to 30 ft." },
  { id: "scorching-ray", level: 2, school: "Evocation", classes: ["wizard"], concentration: false, ritual: false, time: "1 action", range: "120 ft", description: "Three rays of fire." },
  { id: "hold-person", level: 2, school: "Enchantment", classes: ["wizard", "cleric"], concentration: true, ritual: false, time: "1 action", range: "60 ft", description: "Paralyze a humanoid." },
  { id: "spiritual-weapon", level: 2, school: "Evocation", classes: ["cleric"], concentration: false, ritual: false, time: "1 bonus action", range: "60 ft", description: "A floating weapon attacks for you." },
  { id: "lesser-restoration", level: 2, school: "Abjuration", classes: ["cleric"], concentration: false, ritual: false, time: "1 action", range: "Touch", description: "End a disease or condition." },
  { id: "fireball", level: 3, school: "Evocation", classes: ["wizard"], concentration: false, ritual: false, time: "1 action", range: "150 ft", description: "A burst of flame in a 20-ft radius." },
  { id: "counterspell", level: 3, school: "Abjuration", classes: ["wizard"], concentration: false, ritual: false, time: "1 reaction", range: "60 ft", description: "Interrupt a spell being cast." },
  { id: "dispel-magic", level: 3, school: "Abjuration", classes: ["wizard", "cleric"], concentration: false, ritual: false, time: "1 action", range: "120 ft", description: "End magic on a target." },
  { id: "revivify", level: 3, school: "Necromancy", classes: ["cleric"], concentration: false, ritual: false, time: "1 action", range: "Touch", description: "Return a recently dead creature to life." },
];

export const ITEMS: ItemDef[] = [
  { id: "dagger", category: "Weapons", costCp: 200, weight: 1, weapon: { damage: "1d4 piercing", mastery: "Nick", properties: "Finesse, light, thrown", range: "20/60" }, description: "A simple blade." },
  { id: "handaxe", category: "Weapons", costCp: 500, weight: 2, weapon: { damage: "1d6 slashing", mastery: "Vex", properties: "Light, thrown", range: "20/60" }, description: "A small throwing axe." },
  { id: "mace", category: "Weapons", costCp: 500, weight: 4, weapon: { damage: "1d6 bludgeoning", mastery: "Sap", properties: "" }, description: "A flanged club." },
  { id: "spear", category: "Weapons", costCp: 100, weight: 3, weapon: { damage: "1d6 piercing", mastery: "Sap", properties: "Thrown, versatile" }, description: "A sturdy polearm." },
  { id: "longsword", category: "Weapons", costCp: 1500, weight: 3, weapon: { damage: "1d8 slashing", mastery: "Sap", properties: "Versatile (1d10)", martial: true }, description: "A balanced martial blade." },
  { id: "greatsword", category: "Weapons", costCp: 5000, weight: 6, weapon: { damage: "2d6 slashing", mastery: "Graze", properties: "Heavy, two-handed", martial: true }, description: "A massive two-handed sword." },
  { id: "rapier", category: "Weapons", costCp: 2500, weight: 2, weapon: { damage: "1d8 piercing", mastery: "Vex", properties: "Finesse", martial: true }, description: "A slender thrusting sword." },
  { id: "shortbow", category: "Weapons", costCp: 2500, weight: 2, weapon: { damage: "1d6 piercing", mastery: "Vex", properties: "Ammunition, two-handed", range: "80/320" }, description: "A compact bow." },
  { id: "leather-armor", category: "Armor", costCp: 1000, weight: 10, armor: { base: 11, dexCap: null }, description: "Light boiled leather." },
  { id: "chain-shirt", category: "Armor", costCp: 5000, weight: 20, armor: { base: 13, dexCap: 2 }, description: "Interlocking rings under clothing." },
  { id: "chain-mail", category: "Armor", costCp: 7500, weight: 55, armor: { base: 16, dexCap: 0, strength: 13, stealth: true }, description: "Heavy mail armor." },
  { id: "plate-armor", category: "Armor", costCp: 150000, weight: 65, armor: { base: 18, dexCap: 0, strength: 15, stealth: true }, description: "Full plate harness." },
  { id: "shield", category: "Armor", costCp: 1000, weight: 6, armor: { base: 2, dexCap: 0, shield: true }, description: "A wooden or metal shield." },
  { id: "backpack", category: "Adventuring Gear", costCp: 200, weight: 5, description: "Carries up to 30 lb." },
  { id: "rope", category: "Adventuring Gear", costCp: 100, weight: 10, description: "Fifty feet of hempen rope." },
  { id: "torch", category: "Adventuring Gear", costCp: 1, weight: 1, description: "Burns for one hour." },
  { id: "rations", category: "Adventuring Gear", costCp: 50, weight: 2, description: "One day of travel food." },
  { id: "healers-kit", category: "Tools", costCp: 500, weight: 3, description: "Stabilize the dying." },
  { id: "thieves-tools", category: "Tools", costCp: 2500, weight: 1, description: "Picks and files." },
  { id: "spellbook", category: "Adventuring Gear", costCp: 5000, weight: 3, description: "Holds a wizard's spells." },
  { id: "holy-symbol", category: "Adventuring Gear", costCp: 500, weight: 1, description: "A focus for divine magic." },
  { id: "explorers-pack", category: "Equipment Packs", costCp: 1000, weight: 59, pack: ["backpack", "rope", "torch", "rations"], description: "Essentials for a dungeon delve." },
  { id: "dungeoneers-pack", category: "Equipment Packs", costCp: 1200, weight: 10, pack: ["backpack", "rope", "torch", "rations"], description: "Includes a backpack, a crowbar, a hammer, 10 pitons, 10 torches, a tinderbox, 10 days of rations, and a waterskin." },
  { id: "ring-of-protection", category: "Magic Items", costCp: 0, weight: 0, attune: true, bonusAc: 1, description: "+1 to AC and saving throws." },
  { id: "cloak-of-protection", category: "Magic Items", costCp: 0, weight: 1, attune: true, bonusAc: 1, description: "+1 to AC and saving throws." },
  { id: "amulet-of-vigor", category: "Magic Items", costCp: 0, weight: 1, attune: true, description: "Warm to the touch; steadies the heart." },
  { id: "wand-of-sparks", category: "Magic Items", costCp: 0, weight: 1, attune: true, description: "Crackling wand with 7 charges." },
  { id: "staff-of-healing", category: "Staffs", costCp: 0, weight: 4, attune: true, attuneBy: { text: "by a bard, cleric, or druid", classes: ["bard", "cleric", "druid"] }, description: "Spend its charges to restore hit points." },
  { id: "boots-of-speed", category: "Wondrous Items", costCp: 0, weight: 1, attune: true, speedToggle: 2, description: "Click the heels together to double your walking speed." },
  { id: "magic-arrows", category: "Ammunition", costCp: 0, weight: 1, ammoBonus: 1, description: "Twenty +1 arrows: +1 to attack and damage rolls made with them." },
  { id: "necklace-of-beads", category: "Wondrous Items", costCp: 0, weight: 0, charges: { dice: "1d6 + 3", count: 1, sides: 6, bonus: 3 }, description: "Its beads (1d6 + 3) are counted when it is found; detach one and throw it." },
  { id: "potion-of-healing", category: "Magic Items", costCp: 5000, weight: 0.5, description: "Restore 2d4+2 hit points." },
  // B-55 W3: one name in three sources and both editions (one add-item row, three versions), a 2014-only weapon and an
  // imported 2024 weapon (Weapon Mastery lists the character's edition and honours source restrictions).
  { id: "hooded-lantern", name: "Hooded Lantern", rulesets: ["dnd2014"], category: "Adventuring Gear", costCp: 500, weight: 2, description: "Sheds bright light in a 30-foot radius." },
  { id: "hooded-lantern-2024", name: "Hooded Lantern", source: SOURCE_2024, rulesets: ["dnd2024"], category: "Adventuring Gear", costCp: 500, weight: 2, description: "Burns oil for 6 hours of light." },
  { id: "hooded-lantern-expansion", name: "Hooded Lantern", source: SOURCE_EXPANSION, rulesets: ["dnd2024"], category: "Adventuring Gear", costCp: 700, weight: 2, description: "A shuttered lantern with a brass hood." },
  { id: "trident", name: "Trident", rulesets: ["dnd2014"], category: "Weapons", costCp: 500, weight: 4, weapon: { damage: "1d6 piercing", properties: "Thrown, versatile", range: "20/60" }, description: "A three-pronged spear." },
  // A magic weapon (no mastery property): never a Weapon Mastery choice.
  { id: "flame-tongue", name: "Flame Tongue", category: "Weapons", costCp: 0, weight: 3, attune: true, weapon: { damage: "1d8 slashing", properties: "Versatile (1d10)", martial: true }, description: "A sword that bursts into flame." },
  { id: "whip-expansion", name: "Whip", source: SOURCE_EXPANSION, rulesets: ["dnd2024"], category: "Weapons", costCp: 200, weight: 3, weapon: { damage: "1d4 slashing", mastery: "Slow", properties: "Finesse, reach", martial: true }, description: "A braided leather lash." },
];

/** R6: subclasses written for one edition only, so the other edition's pickers show the edition marks. */
const ONE_EDITION_SUBCLASS: Record<string, Partial<MockElement>> = {
  "light-domain": { rulesets: ["dnd2014"] },
  "shadow-blade": { rulesets: ["dnd2024"], source: SOURCE_2024 },
};

function mk(): MockElement[] {
  const out: MockElement[] = [];
  for (const r of RACES) {
    const [s, f] = raceText[r.id]!;
    out.push(el(r.id, "race", title(r.id), s, { description: `<p>${s}</p><p>${f}</p>`, facts: [{ label: "Speed", value: `${r.speed} ft` }], ...RACE_EXTRA[r.id] }));
    for (const sub of r.subraces) {
      if (out.some((e) => e.id === sub)) continue;
      const [ss, sf] = raceText[sub]!;
      const name = sub.startsWith("ancestry-") ? `${title(sub.split("-")[1]!)} Ancestry` : title(sub);
      out.push(el(sub, "subrace", name, ss, { description: `<p>${ss}</p><p>${sf}</p>`, ...RACE_EXTRA[sub] }));
    }
  }
  for (const c of CLASSES) {
    out.push(el(c.id, "class", title(c.id), classText[c.id]!, {
      facts: [{ label: "Hit die", value: `d${c.hitDie}` }, { label: "Saves", value: c.saves.join(", ").toUpperCase() }],
      description: `<p>${classText[c.id]}</p><ul><li>Hit die d${c.hitDie}</li><li>Choose ${c.skillCount} skills</li></ul>`,
    }));
    out.push(el(`multi-${c.id}`, "multiclass", title(c.id), classText[c.id]!, {
      prerequisites: `${c.multiclassAbility.map((a) => ABILITY_NAMES[a]).join(" or ")} 13`,
    }));
    for (const s of c.subclasses) out.push(el(s, "archetype", title(s), subText[s]!, ONE_EDITION_SUBCLASS[s] ?? {}));
  }
  for (const b of BACKGROUNDS) {
    out.push(el(b.id, "background", title(b.id), `Skills: ${b.skills.join(", ")}.`, {
      description: `<p>A background of ${title(b.id).toLowerCase()} experience.</p>`,
      grants: b.skills.map((k) => `Proficiency: ${k}`),
    }));
  }
  out.push(...FEATS, ...FIGHTING_STYLES);
  for (const group of Object.values(LISTS)) out.push(...group);
  for (const s of SPELLS) {
    out.push(el(s.id, "spell", title(s.id), s.description, {
      level: s.level, school: s.school, tags: s.classes, spell: spellFacts(s),
      description: `<p>${s.description}</p>`,
      facts: [{ label: "Casting time", value: s.time }, { label: "Range", value: s.range }, { label: "School", value: s.school }, { label: "Level", value: s.level === 0 ? "Cantrip" : String(s.level) }],
    }));
  }
  for (const i of ITEMS) {
    const type: ElementType = i.weapon ? "weapon" : i.armor ? "armor" : i.category === "Magic Items" ? "magic-item" : "item";
    out.push(el(i.id, type, i.name ?? title(i.id).replace("Healers", "Healer's").replace("Explorers", "Explorer's").replace("Dungeoneers", "Dungeoneer's").replace("Thieves", "Thieves'"), i.description, {
      ...(i.source ? { source: i.source } : {}), ...(i.rulesets ? { rulesets: i.rulesets } : {}),
      tags: [i.category], armor: armorFacts(i), weapon: weaponFacts(i),
      facts: [{ label: "Cost", value: `${i.costCp / 100} gp` }, { label: "Weight", value: `${i.weight} lb` }, ...(i.weapon ? [{ label: "Damage", value: i.weapon.damage }] : [])],
    }));
  }
  for (const k of SKILLS) out.push(el(`skill-${k.key}`, "skill", k.name, `${ABILITY_NAMES[k.ability]} skill.`));
  for (const l of LANGUAGES) out.push(el(`lang-${l.toLowerCase()}`, "language", l, `The ${l} language.`));
  for (const a of ["str", "dex", "con", "int", "wis", "cha"] as AbilityKey[]) {
    out.push(el(`asi:${a}:2`, "asi", `+2 ${ABILITY_NAMES[a]}`, `Increase ${ABILITY_NAMES[a]} by 2.`));
    out.push(el(`asi:${a}:1`, "asi", `+1 ${ABILITY_NAMES[a]}`, `Increase ${ABILITY_NAMES[a]} by 1.`, { rulesets: ["dnd2024"], source: SOURCE_2024 }));
  }
  out.push(...monsterElements((id, name, short, extra) => el(id, "monster", name, short, extra)));
  // Items that carry the same id in 2024 sample get the 2024 source for a few new entries
  return out;
}

export const ELEMENTS: MockElement[] = mk();
export const ELEMENT_BY_ID = new Map(ELEMENTS.map((e) => [e.id, e]));
export const getElement = (id: string): MockElement | undefined => ELEMENT_BY_ID.get(id);
export const getRace = (id: string) => RACES.find((r) => r.id === id);
export const getClass = (id: string | null) => CLASSES.find((c) => c.id === id) ?? null;
export const getBackground = (id: string) => BACKGROUNDS.find((b) => b.id === id);
export const getItem = (id: string) => ITEMS.find((i) => i.id === id);
export const getSpell = (id: string) => SPELLS.find((s) => s.id === id);
export const titleCase = title;
