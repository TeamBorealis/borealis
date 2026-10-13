import type { AbilityState, CharacterStats, ClassLevelInfo, LevelState, SkillEntry } from "../types";
import { SKILLS, getBackground, getClass, getElement, getItem, getRace, titleCase } from "./content";
import { classIdOf } from "./slots";
import { type MockCharacter, XP_THRESHOLDS, levelOf, totalLevel } from "./state";

export const profBonus = (level: number): number => 2 + Math.floor((Math.max(1, level) - 1) / 4);

/** Per-level hit die values actually used (level 1 of the main class is the fixed maximum). */
export function resolvedHpRolls(c: MockCharacter, index: number): number[] {
  const cs = c.classes[index]!;
  const cls = getClass(classIdOf(c, cs.slotId));
  const die = cls?.hitDie ?? 8;
  const avg = Math.floor(die / 2) + 1;
  return Array.from({ length: levelOf(c, cs.slotId) }, (_, n) => {
    if (index === 0 && n === 0) return die;
    const v = cs.hpRolls[n];
    return v !== undefined && v >= 1 && v <= die ? v : avg;
  });
}

export function deriveClasses(c: MockCharacter): ClassLevelInfo[] {
  return c.classes.map((cs, i) => {
    const cls = getClass(classIdOf(c, cs.slotId));
    const sub = c.choices[`${cs.slotId}:subclass`];
    const subEl = sub ? getElement(sub) : undefined;
    return {
      slotId: cs.slotId, classId: cls?.id ?? null, name: cls ? titleCase(cls.id) : null,
      level: levelOf(c, cs.slotId), hitDie: cls?.hitDie ?? 8, isMain: i === 0,
      subclass: subEl ? { id: subEl.id, name: subEl.name, type: subEl.type, source: subEl.source } : null,
      hpRolls: resolvedHpRolls(c, i), multiclassPrerequisite: i === 0 || !cls ? null : `${cls.multiclassAbility.join("/").toUpperCase()} 13`,
    };
  });
}

export function deriveLevel(c: MockCharacter): LevelState {
  const total = totalLevel(c);
  const next = total >= 20 ? null : XP_THRESHOLDS[total]!;
  const hasMainClass = !!classIdOf(c, c.classes[0]!.slotId);
  const emptyMulti = c.classes.some((cs, i) => i > 0 && !classIdOf(c, cs.slotId));
  const canMulticlass = c.options.multiclassing && total < 20 && hasMainClass && !emptyMulti;
  const reason = !c.options.multiclassing ? "Multiclassing is disabled in the character options."
    : total >= 20 ? "Maximum level reached." : !hasMainClass ? "Choose a main class first."
    : emptyMulti ? "Choose a class for your empty multiclass slot first." : null;
  return {
    total, xp: c.xp, nextLevelXp: next, currentLevelXp: XP_THRESHOLDS[total - 1] ?? 0,
    levelUpAvailable: next !== null && c.xp >= next, canLevelUp: total < 20, canLevelDown: total > 1,
    canMulticlass, multiclassBlockedReason: reason, classes: deriveClasses(c),
  };
}

export function hitPoints(c: MockCharacter, abilities: AbilityState): number {
  const conMod = abilities.modifier.con;
  let hp = 0;
  c.classes.forEach((cs, i) => {
    const cls = getClass(classIdOf(c, cs.slotId));
    if (!cls) return;
    const avg = Math.floor(cls.hitDie / 2) + 1;
    resolvedHpRolls(c, i).forEach((r, n) => {
      const used = i === 0 && n === 0 ? r : c.options.averageHp ? avg : r;
      hp += Math.max(1, used + conMod);
    });
  });
  const total = totalLevel(c);
  if (c.choices["subrace:0"] === "hill-dwarf") hp += total;
  if (Object.values(c.choices).includes("tough")) hp += total * 2;
  return hp;
}

export function armorClass(c: MockCharacter, abilities: AbilityState): number {
  const dex = abilities.modifier.dex;
  let base = 10 + dex;
  let shield = 0;
  let bonus = 0;
  let armored = false;
  for (const it of c.items) {
    const def = getItem(it.itemId);
    if (!def || !it.equipped || it.storage !== 0) continue;
    if (def.armor && !def.armor.shield && it.equipped === "worn") {
      const d = def.armor.dexCap === null ? dex : Math.min(dex, def.armor.dexCap);
      base = def.armor.base + d;
      armored = true;
    } else if (def.armor?.shield) shield = def.armor.base;
    if (def.bonusAc && (!def.attune || it.attuned)) bonus += def.bonusAc;
  }
  const defense = armored && Object.values(c.choices).includes("defense") ? 1 : 0;
  return base + shield + bonus + defense;
}

export function proficientSkills(c: MockCharacter): Set<string> {
  const set = new Set<string>();
  const bg = getBackground(c.choices["bg:background"] ?? "");
  bg?.skills.forEach((s) => set.add(s));
  for (const [slot, el] of Object.entries(c.choices)) if (slot.includes(":skill:") && el.startsWith("skill-")) set.add(el.slice(6));
  return set;
}

export function deriveStats(c: MockCharacter, abilities: AbilityState): CharacterStats {
  const level = totalLevel(c);
  const prof = profBonus(level);
  const profs = proficientSkills(c);
  const skills: SkillEntry[] = SKILLS.map((s) => ({
    key: s.key, name: s.name, ability: s.ability,
    bonus: abilities.modifier[s.ability] + (profs.has(s.key) ? prof : 0),
    proficiency: profs.has(s.key) ? "proficient" : "none",
  }));
  const race = getRace(c.choices["race:0"] ?? "");
  const speed = (race?.speed ?? 30) + (c.choices["subrace:0"] === "wood-elf" ? 5 : 0);
  const per = skills.find((s) => s.key === "perception")!;
  const langs = new Set<string>(["Common"]);
  if (race?.id === "elf") langs.add("Elvish");
  if (race?.id === "dwarf") langs.add("Dwarvish");
  if (race?.id === "halfling") langs.add("Halfling");
  for (const [slot, el] of Object.entries(c.choices)) if (el.startsWith("lang-") && slot !== "") langs.add(titleCase(el.slice(5)));
  const main = getClass(classIdOf(c, c.classes[0]!.slotId));
  const featureTypes = new Set(["archetype", "fighting-style", "feat", "subrace"]);
  const features = Object.values(c.choices)
    .map((id) => getElement(id))
    .filter((e): e is NonNullable<typeof e> => !!e && featureTypes.has(e.type))
    .map((e) => ({ name: e.name, source: e.sourceName }));
  return {
    proficiencyBonus: prof, armorClass: armorClass(c, abilities), initiative: abilities.modifier.dex, speed,
    hpMax: hitPoints(c, abilities), passivePerception: 10 + per.bonus, skills, languages: [...langs],
    proficiencies: main ? [
      { label: "Saving throws", values: main.saves.map((s) => s.toUpperCase()) },
      { label: "Armor", values: main.id === "fighter" ? ["All armor", "Shields"] : main.id === "cleric" ? ["Light", "Medium", "Shields"] : main.id === "rogue" ? ["Light"] : [] },
    ] : [],
    features,
  };
}

