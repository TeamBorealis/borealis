import { type AbilityMethod, type CharacterOptions, type NewCharacterRequest } from "../types";
import { EMPTY_DETAILS, type MockCharacter, noAssign, uniformScores } from "./state";
import { makeItem } from "./ops-equipment";
import { reprocess } from "./slots";

export const DEFAULT_OPTIONS: CharacterOptions = {
  multiclassing: true, feats: true, averageHp: true, pointBuyUnrestricted: false, attunementLimit: 3, custom: {},
};

let seq = 0;
export const newId = (): string => `char-${Date.now().toString(36)}-${++seq}`;

export function baseScoresFor(method: AbilityMethod) {
  return method === "point_buy" || method === "standard_array" ? uniformScores(8) : uniformScores(10);
}

export function createCharacter(req: NewCharacterRequest, defaults: { playerName: string }): MockCharacter {
  const level = Math.min(20, Math.max(1, Math.round(req.startingLevel)));
  const c: MockCharacter = {
    id: newId(), name: req.name.trim() || "Unnamed Hero", ruleset: req.ruleset, group: req.group ?? "Characters", favorite: false,
    portrait: req.portrait ?? null, path: null, updatedAt: new Date().toISOString(), dirty: true, xp: 0,
    options: { ...DEFAULT_OPTIONS, ...req.options, custom: { ...(req.options.custom ?? {}) } },
    method: req.abilityMethod, base: baseScoresFor(req.abilityMethod), arrayAssign: noAssign(), rolls: [],
    classes: [{ slotId: "c0", hpRolls: Array.from({ length: level }, () => 0) }], levelOrder: Array.from({ length: level }, () => "c0"),
    choices: {}, prepared: {}, items: [], coins: { cp: 0, sp: 0, ep: 0, gp: 150, pp: 0 }, storageNames: ["Backpack", "Storage 2"],
    details: { ...EMPTY_DETAILS, gender: req.gender, playerName: defaults.playerName }, attacks: [], companionName: "", companionPortrait: null,
    sourceRestrictions: null, notices: [], warnings: [],
  };
  return c;
}

/** Applies picks by slot id and reprocesses; used to seed the sample library. */
export function withChoices(c: MockCharacter, choices: Record<string, string>): MockCharacter {
  return reprocess({ ...c, choices: { ...c.choices, ...choices } }).character;
}

export function seedLibrary(): MockCharacter[] {
  let fighter = createCharacter({ name: "Brannoc Ironvale", ruleset: "dnd2014", startingLevel: 3, gender: "Male", abilityMethod: "manual", options: {} }, { playerName: "Sample Player" });
  fighter = { ...fighter, base: { str: 16, dex: 12, con: 15, int: 10, wis: 13, cha: 8 }, xp: 900, group: "Campaign: Ashfall", favorite: true, dirty: false, path: "/sample/brannoc.json" };
  fighter = withChoices(fighter, { "race:0": "dwarf", "subrace:0": "hill-dwarf", "c0:class": "fighter", "bg:background": "soldier",
    "c0:skill:0": "skill-perception", "c0:skill:1": "skill-survival", "c0:style:0": "defense", "c0:subclass": "champion" });
  fighter = { ...fighter, items: [{ ...makeItem("longsword", 1), equipped: "primary" }, { ...makeItem("chain-mail", 1), equipped: "worn" }, { ...makeItem("shield", 1), equipped: "secondary" }] };

  let wizard = createCharacter({ name: "Seraphine Vale", ruleset: "dnd2024", startingLevel: 2, gender: "Female", abilityMethod: "manual", options: {} }, { playerName: "Sample Player" });
  wizard = { ...wizard, base: { str: 8, dex: 14, con: 13, int: 15, wis: 12, cha: 10 }, xp: 300, dirty: false, path: "/sample/seraphine.json" };
  wizard = withChoices(wizard, { "race:0": "elf", "subrace:0": "high-elf", "c0:class": "wizard", "bg:background": "sage", "bg:asi:2": "asi:int:2", "bg:asi:1": "asi:con:1",
    "c0:skill:0": "skill-insight", "c0:skill:1": "skill-investigation", "lang:0": "lang-giant", "lang:1": "lang-draconic",
    "c0:cantrip:0": "fire-bolt", "c0:cantrip:1": "mage-hand", "c0:cantrip:2": "light",
    "c0:spell:0": "magic-missile", "c0:spell:1": "shield", "c0:spell:2": "sleep", "c0:spell:3": "mage-armor",
    "c0:spell:4": "detect-magic", "c0:spell:5": "identify", "c0:spell:6": "thunderwave", "c0:spell:7": "feather-fall" });
  wizard = { ...wizard, prepared: { c0: ["magic-missile", "shield", "mage-armor", "thunderwave"] }, items: [{ ...makeItem("spellbook", 1) }, { ...makeItem("dagger", 1) }, { ...makeItem("explorers-pack", 1) }] };
  return [fighter, wizard];
}
