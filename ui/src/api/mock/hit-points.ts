import type { PlayHitDice, PlayHitPoints, Ruleset } from "../types";
import { apiError } from "./errors";

/** The mock's hit points (B-194), kept on `MockPlay.hp`: as the host, a loss rather than a current value. */
export interface MockHitPoints {
  lost: number;
  temp: number;
  /** Spent hit dice per class id. */
  diceUsed: Record<string, number>;
}

export const MAX_HP_CHANGE = 9999;
export const emptyHitPoints = (): MockHitPoints => ({ lost: 0, temp: 0, diceUsed: {} });

/** Current, maximum and temporary hit points, as the host. */
export function hitPointsOf(h: MockHitPoints | undefined, max: number): PlayHitPoints {
  const hp = h ?? emptyHitPoints();
  return { max, current: Math.max(0, max - hp.lost), temp: hp.temp };
}

function checked(amount: number, what: string): number {
  if (!Number.isInteger(amount) || amount < 1 || amount > MAX_HP_CHANGE) throw apiError("invalid_argument", `${what} is a whole number from 1 to ${MAX_HP_CHANGE}`);
  return amount;
}

/** As the host: temporary hit points first, then hit points, which stop at 0. */
export function takeDamage(h: MockHitPoints | undefined, max: number, amount: number): MockHitPoints {
  const hp = h ?? emptyHitPoints();
  const n = checked(amount, "Damage");
  const fromTemp = Math.min(n, hp.temp);
  return { ...hp, temp: hp.temp - fromTemp, lost: Math.min(max, Math.min(hp.lost, max) + n - fromTemp) };
}

/** As the host: never above the maximum; temporary hit points untouched. */
export function regain(h: MockHitPoints | undefined, max: number, amount: number): MockHitPoints {
  const hp = h ?? emptyHitPoints();
  return { ...hp, lost: Math.max(0, Math.min(hp.lost, max) - checked(amount, "Healing")) };
}

/** As the host: replaces them (they don't stack); 0 ends them. */
export function setTemp(h: MockHitPoints | undefined, amount: number): MockHitPoints {
  if (!Number.isInteger(amount) || amount < 0 || amount > MAX_HP_CHANGE) throw apiError("invalid_argument", `Temporary hit points are a whole number from 0 to ${MAX_HP_CHANGE}`);
  return { ...(h ?? emptyHitPoints()), temp: amount };
}

const withUsed = (hp: MockHitPoints, classId: string, used: number): MockHitPoints => {
  const { [classId]: _old, ...rest } = hp.diceUsed;
  return { ...hp, diceUsed: used > 0 ? { ...rest, [classId]: used } : rest };
};

function diceOf(dice: PlayHitDice[], classId: string): PlayHitDice {
  const d = dice.find((x) => x.classId === classId);
  if (!d) throw apiError("not_found", `Unknown element ${classId}`);
  return d;
}

/** As the host: one die spent heals the roll plus the Constitution modifier (2024: at least 1, 2014: at least 0). */
export function spendHitDie(h: MockHitPoints | undefined, dice: PlayHitDice[], o: { classId: string; roll: number; con: number; max: number; ruleset: Ruleset }): MockHitPoints {
  const hp = h ?? emptyHitPoints();
  const d = diceOf(dice, o.classId);
  if (d.used >= d.total) throw apiError("requirement_failed", `${d.className} has no hit dice left`);
  if (!Number.isInteger(o.roll) || o.roll < 1 || o.roll > d.die) throw apiError("invalid_argument", `A d${d.die} rolls 1 to ${d.die}`);
  const healed = Math.max(o.ruleset === "dnd2024" ? 1 : 0, o.roll + o.con);
  return withUsed({ ...hp, lost: Math.max(0, Math.min(hp.lost, o.max) - healed) }, o.classId, d.used + 1);
}

export function restoreHitDie(h: MockHitPoints | undefined, dice: PlayHitDice[], classId: string, n: number): MockHitPoints {
  const hp = h ?? emptyHitPoints();
  const d = diceOf(dice, classId);
  return withUsed(hp, classId, Math.max(0, d.used - n));
}

/** As the host's Long Rest: all hit points back, temporary ones gone; 2024 every spent die, 2014 up to half the total (at least one), largest dice first. */
export function afterLongRest(h: MockHitPoints | undefined, dice: PlayHitDice[], ruleset: Ruleset): MockHitPoints {
  if (ruleset === "dnd2024") return emptyHitPoints();
  let left = Math.max(1, Math.floor(dice.reduce((n, d) => n + d.total, 0) / 2));
  const diceUsed: Record<string, number> = {};
  for (const d of [...dice].sort((a, b) => b.die - a.die || a.className.localeCompare(b.className))) {
    const back = Math.min(d.used, left);
    left -= back;
    if (d.used > back) diceUsed[d.classId] = d.used - back;
  }
  return { ...(h ?? emptyHitPoints()), lost: 0, temp: 0, diceUsed };
}
