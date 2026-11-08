import { ELEMENTS, getClass } from "./content";
import { apiError } from "./errors";
import { classIdOf } from "./slots";
import { type MockCharacter, XP_THRESHOLDS, levelOf, totalLevel } from "./state";

export function levelUp(c: MockCharacter, classSlotId: string): MockCharacter {
  if (totalLevel(c) >= 20) throw apiError("requirement_failed", "Maximum level reached.");
  const idx = c.classes.findIndex((x) => x.slotId === classSlotId);
  if (idx < 0) throw apiError("not_found", "Unknown class slot.");
  if (idx > 0 && !classIdOf(c, classSlotId)) throw apiError("requirement_failed", "Choose a class for this slot first.");
  const classes = c.classes.map((x) => (x.slotId === classSlotId ? { ...x, hpRolls: [...x.hpRolls, 0] } : x));
  return { ...c, classes, levelOrder: [...c.levelOrder, classSlotId] };
}

export function levelDown(c: MockCharacter): MockCharacter {
  if (totalLevel(c) <= 1) throw apiError("requirement_failed", "The first level cannot be removed.");
  const slotId = c.levelOrder[c.levelOrder.length - 1]!;
  const levelOrder = c.levelOrder.slice(0, -1);
  let classes = c.classes.map((x) => (x.slotId === slotId ? { ...x, hpRolls: x.hpRolls.slice(0, -1) } : x));
  const stillHas = levelOrder.includes(slotId);
  if (!stillHas && slotId !== c.classes[0]!.slotId) classes = classes.filter((x) => x.slotId !== slotId);
  const choices = Object.fromEntries(Object.entries(c.choices).filter(([k]) => stillHas || !k.startsWith(`${slotId}:`)));
  // XP is set to the new level's threshold.
  return { ...c, classes, levelOrder, choices, xp: XP_THRESHOLDS[levelOrder.length - 1] ?? 0 };
}

export function addMulticlass(c: MockCharacter): MockCharacter {
  if (!c.options.multiclassing) throw apiError("requirement_failed", "Multiclassing is disabled in the character options.");
  if (totalLevel(c) >= 20) throw apiError("requirement_failed", "Maximum level reached.");
  if (!classIdOf(c, c.classes[0]!.slotId)) throw apiError("requirement_failed", "Choose a main class first.");
  const next = Math.max(...c.classes.map((x) => Number(x.slotId.slice(1)))) + 1;
  const slotId = `c${next}`;
  return { ...c, classes: [...c.classes, { slotId, hpRolls: [0] }], levelOrder: [...c.levelOrder, slotId] };
}

/** `order` without its last `n` entries of `slotId`. */
function withoutLast(order: string[], slotId: string, n: number): string[] {
  const dropAt = new Set(order.map((s, i) => (s === slotId ? i : -1)).filter((i) => i >= 0).slice(-n));
  return order.filter((_, i) => !dropAt.has(i));
}

/** Move the last `n` levels of `from` to the end of the level order as levels of `to` (B2a); a multiclass left without levels goes, with its choices. */
function moveLevels(c: MockCharacter, from: string, to: string, n: number): MockCharacter {
  const levelOrder = [...withoutLast(c.levelOrder, from, n), ...Array<string>(n).fill(to)];
  const kept = c.classes.filter((x, i) => i === 0 || levelOrder.includes(x.slotId));
  const gone = c.classes.filter((x) => !kept.includes(x)).map((x) => `${x.slotId}:`);
  const classes = kept.map((x) => ({ ...x, hpRolls: Array.from({ length: levelOrder.filter((s) => s === x.slotId).length }, (_, i) => x.hpRolls[i] ?? 0) }));
  const choices = Object.fromEntries(Object.entries(c.choices).filter(([k]) => !gone.some((g) => k.startsWith(g))));
  return { ...c, classes, levelOrder, choices };
}

function checkMainKeeps(c: MockCharacter, taken: number): void {
  const main = levelOf(c, c.classes[0]!.slotId);
  if (taken >= main) throw apiError("requirement_failed", `The main class has ${main} levels and keeps at least one.`);
}

/** B2a: a new multiclass from the main class's latest `levels` levels; `elementId` is its class pick. */
export function splitMulticlass(c: MockCharacter, levels: number, elementId?: string): MockCharacter {
  if (!c.options.multiclassing) throw apiError("requirement_failed", "Multiclassing is disabled in the character options.");
  if (!classIdOf(c, c.classes[0]!.slotId)) throw apiError("requirement_failed", "Choose a main class first.");
  if (levels < 1) throw apiError("requirement_failed", "A new multiclass needs at least one level.");
  checkMainKeeps(c, levels);
  const taken = new Set(c.classes.map((x) => classIdOf(c, x.slotId)));
  const pick = elementId ? ELEMENTS.find((e) => e.id === elementId && e.type === "multiclass") : undefined;
  if (elementId && (!pick || taken.has(pick.id.slice(6)))) throw apiError("requirement_failed", "That option is not available for this slot.");
  const slotId = `c${Math.max(...c.classes.map((x) => Number(x.slotId.slice(1)))) + 1}`;
  const moved = moveLevels({ ...c, classes: [...c.classes, { slotId, hpRolls: [] }] }, c.classes[0]!.slotId, slotId, levels);
  return elementId ? { ...moved, choices: { ...moved.choices, [`${slotId}:multiclass`]: elementId } } : moved;
}

/** B2a: give a multiclass exactly `levels` levels, moved from or back to the main class (0 removes it). */
export function setClassLevels(c: MockCharacter, classSlotId: string, levels: number): MockCharacter {
  const idx = c.classes.findIndex((x) => x.slotId === classSlotId);
  if (idx === 0) throw apiError("invalid_argument", "Set the levels of a multiclass: the main class keeps the rest.");
  if (idx < 0) throw apiError("requirement_failed", "No such multiclass.");
  const main = c.classes[0]!.slotId;
  const cur = levelOf(c, classSlotId);
  if (levels === cur) return c;
  if (levels > cur) {
    checkMainKeeps(c, levels - cur);
    return moveLevels(c, main, classSlotId, levels - cur);
  }
  return moveLevels(c, classSlotId, main, cur - Math.max(0, levels));
}

/** Store a roll, or with `value` null clear it (stored as 0) so the level uses the average. */
export function setHpRoll(c: MockCharacter, classSlotId: string, level: number, value: number | null): MockCharacter {
  if (level < 1 || level > levelOf(c, classSlotId)) throw apiError("invalid_argument", "The character has no such class level.");
  const die = getClass(classIdOf(c, classSlotId))?.hitDie ?? 8;
  if (value !== null && (value < 1 || value > die)) throw apiError("invalid_argument", `A hit point roll must be between 1 and ${die}.`);
  const classes = c.classes.map((x) => {
    if (x.slotId !== classSlotId) return x;
    const rolls = Array.from({ length: Math.max(level, x.hpRolls.length) }, (_, i) => (i === level - 1 ? value ?? 0 : x.hpRolls[i] ?? 0));
    return { ...x, hpRolls: rolls };
  });
  return { ...c, classes };
}
