import type { AbilityState, ClassSpellcasting, SpellEntry, SpellcastingState } from "../types";
import { SPELLS, getClass, getElement, getSpell, titleCase } from "./content";
import { spellFacts } from "./facts";
import { classIdOf } from "./slots";
import { SLOT_TABLE, cantripsAt, casterMaxSpellLevel } from "./spell-tables";
import { type MockCharacter, levelOf, totalLevel } from "./state";
import { profBonus } from "./derive-stats";
import { toWireSlotId } from "./slot-ids";

const ALWAYS_PREPARED: Record<string, string[]> = { "life-domain": ["bless", "cure-wounds"] };

function entry(id: string, over: Partial<SpellEntry>): SpellEntry {
  const s = getSpell(id)!;
  return {
    id, name: getElement(id)?.name ?? titleCase(id), level: s.level, school: s.school, prepared: false, alwaysPrepared: false,
    origin: "class", slotId: null, canReplace: false, concentration: s.concentration, ritual: s.ritual, casting: spellFacts(s), sourceName: getElement(id)?.sourceName ?? "", ...over,
  };
}

export function preparedLimit(abilityMod: number, classLevel: number): number {
  return Math.max(1, abilityMod + classLevel); // Clamped to a minimum of 1
}

export function deriveSpellcasting(c: MockCharacter, ab: AbilityState): SpellcastingState {
  const classes: ClassSpellcasting[] = [];
  let casterLevel = 0;
  for (const cs of c.classes) {
    const cls = getClass(classIdOf(c, cs.slotId));
    if (!cls?.caster) continue;
    const lvl = levelOf(c, cs.slotId);
    casterLevel += lvl;
    const abil = cls.caster.ability;
    const m = ab.modifier[abil];
    const prof = profBonus(totalLevel(c));
    const sub = c.choices[`${cs.slotId}:subclass`] ?? "";
    const always = ALWAYS_PREPARED[sub] ?? [];
    const prepSet = new Set(c.prepared[cs.slotId] ?? []);
    const spells: SpellEntry[] = [];
    const chosenSlots = Object.entries(c.choices).filter(([k]) => k.startsWith(`${cs.slotId}:cantrip:`) || k.startsWith(`${cs.slotId}:spell:`));
    for (const [slotId, id] of chosenSlots) {
      if (!getSpell(id)) continue;
      const isCantrip = slotId.includes(":cantrip:");
      spells.push(entry(id, {
        slotId: toWireSlotId(slotId), canReplace: !isCantrip && cls.caster.model === "spellbook",
        prepared: isCantrip || prepSet.has(id) || always.includes(id), alwaysPrepared: isCantrip,
      }));
    }
    if (cls.caster.model === "prepared") {
      const max = casterMaxSpellLevel(lvl);
      for (const s of SPELLS.filter((x) => x.classes.includes(cls.id) && x.level >= 1 && x.level <= max)) {
        const al = always.includes(s.id);
        spells.push(entry(s.id, { prepared: al || prepSet.has(s.id), alwaysPrepared: al }));
      }
    }
    spells.sort((a, b) => a.level - b.level || a.name.localeCompare(b.name));
    const counted = spells.filter((s) => s.level > 0 && s.prepared && !s.alwaysPrepared).length;
    classes.push({
      classSlotId: cs.slotId, className: titleCase(cls.id), ability: abil, saveDc: 8 + prof + m, attackBonus: prof + m,
      model: cls.caster.model === "spellbook" ? "spellbook" : "prepared", cantripsMax: cantripsAt(cls.id, lvl),
      cantripsKnown: spells.filter((s) => s.level === 0).length, preparedMax: preparedLimit(m, lvl), preparedCount: counted,
      maxSpellLevel: casterMaxSpellLevel(lvl), spells,
    });
  }
  const row = (SLOT_TABLE[Math.min(20, casterLevel)] ?? []).map((total, i) => ({ level: i + 1, total }));
  return { classes, slots: row, multiclassSlots: classes.length >= 2 ? row : null };
}
