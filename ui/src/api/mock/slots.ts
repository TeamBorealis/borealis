import type { BuilderTab, ElementType, SelectionSlot } from "../types";
import {
  ABILITY_NAMES, ELEMENTS, FEATS, FIGHTING_STYLES, LISTS, SPELLS,
  getBackground, getClass, getElement, getRace, titleCase,
} from "./content";
import type { MockElement } from "./data-types";
import { type MockCharacter, levelOf } from "./state";
import { casterMaxSpellLevel, cantripsAt, spellbookSize } from "./spell-tables";

export interface SlotDef extends Omit<SelectionSlot, "chosen" | "gainedOnLevel" | "replaced"> {
  candidates: MockElement[];
  dup: string | null;
  lockedTo: string | null;
}

const ASI_LEVELS = [4, 8, 12, 16, 19];
const RESTRICTED_TYPES: ElementType[] = ["race", "subrace", "archetype", "background", "feat", "spell", "weapon", "armor", "item", "magic-item"];

export const classIdOf = (c: MockCharacter, slotId: string): string | null => {
  const key = slotId === c.classes[0]?.slotId ? `${slotId}:class` : `${slotId}:multiclass`;
  const ch = c.choices[key];
  if (!ch) return null;
  return ch.startsWith("multi-") ? ch.slice(6) : ch;
};

type Draft = Partial<SlotDef> & { id: string; tab: BuilderTab; title: string; elementType: ElementType; candidates: MockElement[] };

function make(d: Draft, ordinal = 1, of = 1): SlotDef {
  return {
    groupId: null, groupLabel: null, classSlotId: null, ordinal, of, required: true, locked: false, requiredLevel: null,
    allowReplace: false, hint: null, dup: null, lockedTo: null, ...d,
  };
}

function many(d: Omit<Draft, "id"> & { id: string }, count: number): SlotDef[] {
  return Array.from({ length: count }, (_, i) => make({ ...d, id: `${d.id}:${i}` }, i + 1, count));
}

/** Cross-edition picks are open (null = ruleset default: strict for 2014, open for 2024). */
const crossEditionOpen = (c: MockCharacter) => c.allowCrossEdition ?? c.ruleset === "dnd2024";
/** Same-edition options; other-edition subclasses too while cross-edition picks are open (B-44, marked per R6). */
const CROSS_EDITION_TYPES: ElementType[] = ["archetype", "race"];
const byRuleset = (c: MockCharacter, list: MockElement[]) =>
  list.filter((e) => e.rulesets.includes(c.ruleset) || (CROSS_EDITION_TYPES.includes(e.type) && crossEditionOpen(c)));

/**
 * A kind of weapon a class's Weapon Mastery can choose (as the engine's `weapon_mastery` filter, SRD 5.2.1): a base weapon of
 * the character's edition with a mastery property (never a magic weapon); a Rogue only one it is proficient with (Simple, or
 * Martial with Finesse or Light).
 */
function masteryKind(c: MockCharacter, classId: string, e: MockElement): boolean {
  const w = e.weapon;
  if (e.type !== "weapon" || !w?.mastery || !e.rulesets.includes(c.ruleset)) return false;
  if (classId !== "rogue" || w.proficiency !== "Martial") return true;
  return w.properties.some((p) => /^(finesse|light)$/i.test(p));
}

function classSlots(c: MockCharacter, index: number): SlotDef[] {
  const cs = c.classes[index]!;
  const out: SlotDef[] = [];
  const isMain = index === 0;
  const group = isMain ? {} : { groupId: cs.slotId, groupLabel: `Multiclass ${index}` };
  if (isMain) {
    out.push(make({ id: `${cs.slotId}:class`, tab: "class", title: "Class", elementType: "class", candidates: byRuleset(c, ELEMENTS.filter((e) => e.type === "class")) }));
  } else {
    const taken = new Set(c.classes.map((x, i) => (i === index ? "" : (c.choices[`${x.slotId}:${i === 0 ? "class" : "multiclass"}`] ?? "").replace("multi-", ""))));
    out.push(make({
      id: `${cs.slotId}:multiclass`, tab: "class", title: "Multiclass", elementType: "multiclass", ...group,
      hint: "Only classes whose ability prerequisite you meet are listed.",
      candidates: ELEMENTS.filter((e) => e.type === "multiclass" && !taken.has(e.id.slice(6))),
    }));
  }
  const cls = getClass(classIdOf(c, cs.slotId));
  if (!cls) return out;
  const lvl = levelOf(c, cs.slotId);
  const base = { ...group };
  if (isMain) {
    out.push(...many({ ...base, id: `${cs.slotId}:skill`, tab: "proficiencies", title: `${titleCase(cls.id)} skill`, elementType: "skill", dup: "skill",
      candidates: ELEMENTS.filter((e) => e.type === "skill" && cls.skills.includes(e.id.slice(6))) }, cls.skillCount));
    if (cls.fightingStyle) out.push(make({ ...base, id: `${cs.slotId}:style:0`, tab: "class", title: "Fighting style", elementType: "fighting-style", candidates: FIGHTING_STYLES }));
    if (c.ruleset === "dnd2024" && cls.masteries > 0) {
      out.push(...many({ ...base, id: `${cs.slotId}:mastery`, tab: "class", title: "Weapon Mastery", elementType: "weapon", dup: `mastery:${cs.slotId}`,
        hint: "Choose weapons whose mastery property you can use.",
        candidates: ELEMENTS.filter((e) => masteryKind(c, cls.id, e)) }, cls.masteries));
    }
  }
  const subLevel = cls.subclassLevel[c.ruleset];
  if (lvl >= subLevel) {
    out.push(make({ ...base, id: `${cs.slotId}:subclass`, tab: "class", title: `${titleCase(cls.id)} subclass`, elementType: "archetype", requiredLevel: subLevel,
      candidates: ELEMENTS.filter((e) => e.type === "archetype" && cls.subclasses.includes(e.id)) }));
  }
  for (const al of ASI_LEVELS.filter((l) => lvl >= l)) {
    const feats = c.options.feats ? FEATS.filter((f) => f.tags?.includes("general") || (c.ruleset === "dnd2014" && f.id === "alert")) : [];
    const asis = ELEMENTS.filter((e) => e.type === "asi");
    out.push(make({ ...base, id: `${cs.slotId}:asi:${al}`, tab: "abilities", title: "Ability Score Improvement", elementType: "asi", requiredLevel: al,
      hint: c.options.feats ? "Increase an ability score or take a feat." : "Increase an ability score.",
      candidates: [...asis, ...feats] }));
  }
  if (cls.caster) {
    const cantrips = cantripsAt(cls.id, lvl);
    const list = SPELLS.filter((s) => s.classes.includes(cls.id));
    out.push(...many({ ...base, classSlotId: cs.slotId, id: `${cs.slotId}:cantrip`, tab: "spells", title: `${titleCase(cls.id)} cantrip`, elementType: "spell", dup: `cantrip:${cs.slotId}`,
      candidates: ELEMENTS.filter((e) => e.type === "spell" && e.level === 0 && list.some((s) => s.id === e.id)) }, cantrips));
    if (cls.caster.model === "spellbook") {
      const max = casterMaxSpellLevel(lvl);
      out.push(...many({ ...base, classSlotId: cs.slotId, id: `${cs.slotId}:spell`, tab: "spells", title: "Spellbook spell", elementType: "spell", dup: `spell:${cs.slotId}`, allowReplace: true,
        hint: `Spell levels 1-${max}`,
        candidates: ELEMENTS.filter((e) => e.type === "spell" && (e.level ?? 0) >= 1 && (e.level ?? 0) <= max && list.some((s) => s.id === e.id)) }, spellbookSize(lvl)));
    }
  }
  return out;
}

function raceSlots(c: MockCharacter): SlotDef[] {
  const out: SlotDef[] = [];
  const species = c.ruleset === "dnd2024" ? "Species" : "Race";
  out.push(make({ id: "race:0", tab: "race", title: species, elementType: "race", candidates: byRuleset(c, ELEMENTS.filter((e) => e.type === "race")) }));
  const race = getRace(c.choices["race:0"] ?? "");
  if (race) {
    const subs = race.subraces.filter(() => c.ruleset === "dnd2014" || race.id === "elf" || race.subTitle);
    if (subs.length) {
      const subTitle = race.subTitle ?? (c.ruleset === "dnd2024" ? "Lineage" : "Subrace");
      out.push(make({ id: "subrace:0", tab: "race", title: subTitle, elementType: "subrace", candidates: subs.map((s) => getElement(s)!) }));
    }
    if (c.ruleset === "dnd2014" && race.id === "human") {
      out.push(make({ id: "race:lang:0", tab: "languages", title: "Extra language", elementType: "language", dup: "lang",
        candidates: ELEMENTS.filter((e) => e.type === "language" && e.name !== "Common") }));
    }
  }
  return out;
}

function backgroundSlots(c: MockCharacter): SlotDef[] {
  const out: SlotDef[] = [];
  out.push(make({ id: "bg:background", tab: "background", title: "Background", elementType: "background", candidates: ELEMENTS.filter((e) => e.type === "background") }));
  const bg = getBackground(c.choices["bg:background"] ?? "");
  if (!bg) return out;
  if (c.ruleset === "dnd2024") {
    const opts = (n: 1 | 2) => bg.asi.map((a) => getElement(`asi:${a}:${n}`)!);
    out.push(make({ id: "bg:asi:2", tab: "background", title: "Background increase +2", elementType: "asi", dup: "bg-asi", candidates: opts(2), hint: `One of ${bg.asi.map((a) => ABILITY_NAMES[a]).join(", ")}` }));
    out.push(make({ id: "bg:asi:1", tab: "background", title: "Background increase +1", elementType: "asi", dup: "bg-asi-1", candidates: opts(1).filter((e) => e.id.split(":")[1] !== (c.choices["bg:asi:2"] ?? "").split(":")[1]) }));
    out.push(make({ id: "bg:feat", tab: "feats", title: "Origin feat", elementType: "feat", lockedTo: bg.originFeat, locked: true, candidates: [getElement(bg.originFeat)!] }));
  } else if (bg.languages > 0) {
    out.push(...many({ id: "bg:lang", tab: "languages", title: "Background language", elementType: "language", dup: "lang", candidates: ELEMENTS.filter((e) => e.type === "language" && e.name !== "Common") }, bg.languages));
  }
  for (const [key, list] of Object.entries(LISTS)) {
    out.push(make({ id: `bg:list:${key}`, tab: "background", title: `Background ${key}`, elementType: "list", required: false, candidates: list }));
  }
  return out;
}

function generalSlots(c: MockCharacter): SlotDef[] {
  if (c.ruleset !== "dnd2024") return [];
  return many({ id: "lang", tab: "languages", title: "Language", elementType: "language", dup: "lang", candidates: ELEMENTS.filter((e) => e.type === "language" && e.name !== "Common") }, 2);
}

function restrict(c: MockCharacter, list: MockElement[]): MockElement[] {
  const allowed = c.sourceRestrictions;
  if (!allowed) return list;
  return list.filter((e) => !RESTRICTED_TYPES.includes(e.type) || allowed.includes(e.source));
}

/** Builds every selection slot for the character from its current choices. */
export function buildSlotDefs(c: MockCharacter): SlotDef[] {
  const all = [...raceSlots(c), ...c.classes.flatMap((_, i) => classSlots(c, i)), ...backgroundSlots(c), ...generalSlots(c)];
  // Remove candidates already chosen by sibling slots of the same duplicate group.
  const chosenByGroup = new Map<string, Map<string, string>>();
  for (const s of all) {
    const ch = c.choices[s.id];
    if (s.dup && ch) {
      const g = chosenByGroup.get(s.dup) ?? new Map<string, string>();
      g.set(s.id, ch);
      chosenByGroup.set(s.dup, g);
    }
  }
  return all.map((s) => {
    const taken = s.dup ? chosenByGroup.get(s.dup) : undefined;
    const base = restrict(c, byRuleset(c, s.candidates));
    const candidates = taken ? base.filter((e) => ![...taken.entries()].some(([sid, eid]) => sid !== s.id && eid === e.id)) : base;
    return { ...s, candidates };
  });
}

/** Re-evaluates choices until stable: drops stale picks and applies locked grants (a sole option is never picked by itself). */
export function reprocess(c: MockCharacter): { character: MockCharacter; defs: SlotDef[] } {
  let cur = c;
  for (let pass = 0; pass < 5; pass++) {
    const defs = buildSlotDefs(cur);
    // As the engine: a source restriction narrows option lists but never removes a stored pick.
    const unrestricted = new Map(buildSlotDefs({ ...cur, sourceRestrictions: null }).map((d) => [d.id, d]));
    const next: Record<string, string> = {};
    for (const d of defs) {
      const ch = cur.choices[d.id];
      const offered = (id: string) => d.candidates.some((e) => e.id === id) || !!unrestricted.get(d.id)?.candidates.some((e) => e.id === id);
      if (d.lockedTo && d.candidates.some((e) => e.id === d.lockedTo)) next[d.id] = d.lockedTo;
      else if (ch && offered(ch)) next[d.id] = ch;
    }
    const same = Object.keys(next).length === Object.keys(cur.choices).length && Object.entries(next).every(([k, v]) => cur.choices[k] === v);
    cur = same ? cur : { ...cur, choices: next };
    if (same) return { character: cur, defs };
  }
  return { character: cur, defs: buildSlotDefs(cur) };
}

