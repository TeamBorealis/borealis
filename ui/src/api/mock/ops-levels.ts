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

export function setHpRoll(c: MockCharacter, classSlotId: string, level: number, value: number): MockCharacter {
  if (level < 1 || level > levelOf(c, classSlotId)) throw apiError("invalid_argument", "No such level.");
  const classes = c.classes.map((x) => {
    if (x.slotId !== classSlotId) return x;
    const rolls = Array.from({ length: Math.max(level, x.hpRolls.length) }, (_, i) => x.hpRolls[i] ?? 0);
    rolls[level - 1] = value;
    return { ...x, hpRolls: rolls };
  });
  return { ...c, classes };
}
