import type { AddedKind, AddedPick, AddPickOption } from "../types";
import { ELEMENTS, FEATS, SPELLS, getClass, getElement, titleCase } from "./content";
import { classIdOf } from "./slots";
import { casterMaxSpellLevel } from "./spell-tables";
import { type MockCharacter, levelOf, totalLevel } from "./state";
import { apiError } from "./errors";

/**
 * B3 (GitHub #35): the mock's builder Add actions. The host keeps an added pick as an equipped, hidden proxy line; the
 * mock keeps the list itself (`MockCharacter.addedPicks`) with the same offering rules: only options the character
 * qualifies for, every option (marked) under "Ignore feat prerequisites", nothing it already has.
 */

/** A mock prerequisite holds: only "Level N+" is checked (as the mock's Additional Feat withholding). */
function prerequisiteHolds(c: MockCharacter, text: string | null | undefined): boolean {
  const level = /^Level (\d+)\+$/.exec(text ?? "")?.[1];
  return !level || totalLevel(c) >= Number(level);
}

/** The class slot whose class is named `className`, and its class id. */
function casterOf(c: MockCharacter, className: string | undefined): { slotId: string; classId: string } | null {
  for (const cs of c.classes) {
    const id = classIdOf(c, cs.slotId);
    if (id && getClass(id)?.caster && titleCase(id) === className) return { slotId: cs.slotId, classId: id };
  }
  return null;
}

interface Candidate { option: AddPickOption; taken: boolean }

const row = (id: string, qualifies: boolean, extra: Partial<AddPickOption> = {}): AddPickOption => {
  const e = getElement(id);
  return { id, name: e?.name ?? titleCase(id), sourceName: e?.sourceName ?? "", prerequisite: e?.prerequisites ?? "", level: null, group: null, qualifies, ...extra };
};

function candidates(c: MockCharacter, kind: AddedKind, spellcasting?: string): Candidate[] {
  const picks = c.addedPicks ?? [];
  const has = (id: string) => picks.some((p) => p.kind === kind && p.elementId === id && (kind !== "spell" || p.spellcasting === spellcasting))
    || (kind !== "spell" && Object.values(c.choices).includes(id));
  switch (kind) {
    case "feat":
      return FEATS.filter((f) => f.rulesets.includes(c.ruleset)).map((f) => ({ option: row(f.id, prerequisiteHolds(c, f.prerequisites)), taken: has(f.id) }));
    case "language":
      return ELEMENTS.filter((e) => e.type === "language").map((e) => ({ option: row(e.id, true), taken: has(e.id) }));
    case "proficiency":
      return ELEMENTS.filter((e) => e.type === "skill").map((e) => ({ option: row(e.id, true, { group: "Skill" }), taken: has(e.id) }));
    case "spell": {
      const caster = casterOf(c, spellcasting);
      if (!caster) return [];
      const max = casterMaxSpellLevel(levelOf(c, caster.slotId));
      const known = new Set(Object.entries(c.choices).filter(([k]) => k.startsWith(`${caster.slotId}:`)).map(([, v]) => v));
      return SPELLS.map((s) => ({
        option: row(s.id, s.classes.includes(caster.classId) && s.level <= max, { level: s.level }),
        taken: has(s.id) || known.has(s.id),
      }));
    }
  }
}

export function listOptions(c: MockCharacter, kind: AddedKind, spellcasting?: string): AddPickOption[] {
  const ignore = !!c.options.ignoreFeatPrerequisites;
  return candidates(c, kind, spellcasting)
    .filter((x) => !x.taken && (x.option.qualifies || ignore))
    .map((x) => x.option)
    .sort((a, b) => (a.level ?? 0) - (b.level ?? 0) || a.name.localeCompare(b.name));
}

export function addPick(c: MockCharacter, kind: AddedKind, elementId: string, spellcasting?: string): MockCharacter {
  if (!getElement(elementId)) throw apiError("not_found", `That ${kind} is not installed.`);
  const option = listOptions(c, kind, spellcasting).find((o) => o.id === elementId);
  if (!option) throw apiError("requirement_failed", `That ${kind} cannot be added to this character.`);
  const picks = c.addedPicks ?? [];
  const n = picks.reduce((m, p) => Math.max(m, Number(p.inventoryId.replace(/^added-/, "")) || 0), 0) + 1;
  const pick: AddedPick = {
    inventoryId: `added-${n}`, kind, elementId, name: option.name, sourceName: option.sourceName,
    spellcasting: kind === "spell" ? spellcasting ?? null : null, applied: true, prerequisitesUnmet: false,
  };
  return { ...c, addedPicks: [...picks, pick] };
}

export function removePick(c: MockCharacter, inventoryId: string): MockCharacter {
  const picks = c.addedPicks ?? [];
  if (!picks.some((p) => p.inventoryId === inventoryId)) throw apiError("requirement_failed", "That is not a pick added in the builder.");
  return { ...c, addedPicks: picks.filter((p) => p.inventoryId !== inventoryId) };
}

/** The snapshot's `addedPicks`: a pick that no longer qualifies is withheld, or applied and marked under the house rule. */
export function derivePicks(c: MockCharacter): AddedPick[] {
  const ignore = !!c.options.ignoreFeatPrerequisites;
  return (c.addedPicks ?? []).map((p) => {
    const qualifies = candidates({ ...c, addedPicks: [] }, p.kind, p.spellcasting ?? undefined).find((x) => x.option.id === p.elementId)?.option.qualifies ?? true;
    return { ...p, applied: qualifies || ignore, prerequisitesUnmet: !qualifies && ignore };
  });
}
