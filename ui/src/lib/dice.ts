/**
 * Dice for the Play tab rolls (GitHub #1, #14): a notation parser (`1d8+3`, `2d6`, `1d4+1d6+2`), critical-hit doubling,
 * advantage and disadvantage, and the breakdown text the result card shows ("d20 [14, 7] +5 = 19, advantage").
 * Every roll takes a `DieRoller`, so tests inject a seeded or scripted die; the app uses `cryptoDie` (unbiased).
 * The numbers rolled against (modifiers, crit range, die floor) always come from the character snapshot.
 */

/** Rolls one die with `sides` faces: an integer 1..sides. */
export type DieRoller = (sides: number) => number;

export interface DiceTerm { kind: "dice"; count: number; sides: number; sign: 1 | -1 }
export interface FlatTerm { kind: "flat"; value: number }
export type Term = DiceTerm | FlatTerm;
export interface DiceExpr { terms: Term[] }

export interface RolledTerm { term: Term; rolls: number[]; value: number }
export interface DiceResult { notation: string; total: number; terms: RolledTerm[]; breakdown: string }

export type RollMode = "normal" | "advantage" | "disadvantage";
export interface D20Result {
  mode: RollMode;
  dice: number[];
  natural: number;
  modifier: number;
  total: number;
  isNat20: boolean;
  isNat1: boolean;
  /** The kept die reached the crit range (attacks). */
  isCrit: boolean;
  breakdown: string;
}

export const MAX_DICE = 100;
export const MAX_SIDES = 1000;
export const MAX_TERMS = 20;
export const MAX_FLAT = 10000;
const MAX_TEXT = 200;
const UINT32_RANGE = 2 ** 32;

export class DiceError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "DiceError";
  }
}

const TOKEN = /^\s*(?:([+-])|(\d*)[dD](\d*)|(\d+))/;

function operand(m: RegExpExecArray, sign: 1 | -1): Term {
  if (m[4] !== undefined) {
    const value = Number(m[4]);
    if (value > MAX_FLAT) throw new DiceError(`A modifier above ${MAX_FLAT}.`);
    return { kind: "flat", value: sign * value };
  }
  const count = m[2] === "" ? 1 : Number(m[2]);
  if (m[3] === "") throw new DiceError("A die needs a number of sides, as in 1d6.");
  const sides = Number(m[3]);
  if (count < 1 || count > MAX_DICE) throw new DiceError(`Roll 1 to ${MAX_DICE} dice at a time.`);
  if (sides < 1 || sides > MAX_SIDES) throw new DiceError(`A die has 1 to ${MAX_SIDES} sides.`);
  return { kind: "dice", count, sides, sign };
}

/** Parses dice notation such as `1d8+3`, `2d6`, `d20` or `1d4+1d6+2`. Throws a `DiceError` on anything else. */
export function parseNotation(text: string): DiceExpr {
  if (text.trim() === "") throw new DiceError("No dice to roll.");
  if (text.length > MAX_TEXT) throw new DiceError("The dice text is too long.");
  const terms: Term[] = [];
  let rest = text;
  let sign: 1 | -1 = 1;
  let expectOperand = true;
  let sawOperator = false;
  while (rest.trim() !== "") {
    const m = TOKEN.exec(rest);
    if (!m) throw new DiceError(`Can't read "${rest.trim()}".`);
    rest = rest.slice(m[0].length);
    if (m[1]) {
      const leading = terms.length === 0 && !sawOperator;
      if (expectOperand && !leading) throw new DiceError("Two operators in a row.");
      sign = m[1] === "-" ? -1 : 1;
      sawOperator = true;
      expectOperand = true;
      continue;
    }
    if (!expectOperand) throw new DiceError("Join the dice with + or -.");
    terms.push(operand(m, sign));
    if (terms.length > MAX_TERMS) throw new DiceError(`At most ${MAX_TERMS} parts.`);
    sign = 1;
    sawOperator = false;
    expectOperand = false;
  }
  if (expectOperand) throw new DiceError("The dice text ends with an operator.");
  return { terms };
}

export function tryParseNotation(text: string): DiceExpr | null {
  try {
    return parseNotation(text);
  } catch {
    return null;
  }
}

const DAMAGE_TERM = /(?<![A-Za-z\d])(\d*d\d+|\d+)(?![A-Za-z\d])/gi;
const DAMAGE_JOIN = /^[\sA-Za-z]*([+-])\s*$/;

/**
 * The dice of a damage text as the snapshot prints it (`1d8+3 slashing + 1d6 fire` -> `1d8+3+1d6`). Damage types are
 * dropped; the sum stops at anything that is not a `+` or `-` join. Null when the text starts with no dice or number.
 */
export function damageDice(text: string): DiceExpr | null {
  const parts: string[] = [];
  let end = 0;
  for (const m of text.matchAll(DAMAGE_TERM)) {
    const gap = text.slice(end, m.index);
    if (parts.length === 0) {
      if (!/^\s*[+-]?\s*$/.test(gap)) return null;
      parts.push(`${gap.trim()}${m[1]}`);
    } else {
      const join = DAMAGE_JOIN.exec(gap);
      if (!join) break;
      parts.push(`${join[1]}${m[1]}`);
    }
    end = (m.index ?? 0) + m[0].length;
  }
  return parts.length === 0 ? null : tryParseNotation(parts.join(""));
}

/** A critical hit: every die rolled twice as many times; modifiers stay as they are. */
export function critical(expr: DiceExpr): DiceExpr {
  return { terms: expr.terms.map((t) => (t.kind === "dice" ? { ...t, count: t.count * 2 } : t)) };
}

const termText = (t: Term): string => (t.kind === "dice" ? `${t.count}d${t.sides}` : String(Math.abs(t.value)));
const termSign = (t: Term): 1 | -1 => (t.kind === "dice" ? t.sign : t.value < 0 ? -1 : 1);

export function formatNotation(expr: DiceExpr): string {
  return expr.terms.map((t, i) => `${termSign(t) < 0 ? "-" : i > 0 ? "+" : ""}${termText(t)}`).join("");
}

function checkedRoll(die: DieRoller, sides: number): number {
  const v = die(sides);
  if (!Number.isInteger(v) || v < 1 || v > sides) throw new DiceError(`A d${sides} rolled ${v}.`);
  return v;
}

const signedText = (n: number): string => (n < 0 ? `-${Math.abs(n)}` : `+${n}`);

export interface RollOptions {
  /** Dice that roll below this count as it (Great Weapon Fighting). */
  floor?: number | null;
  /** A note added to the breakdown ("critical"). */
  label?: string;
}

/** Rolls every term of `expr` and sums them. */
export function rollExpr(expr: DiceExpr, die: DieRoller, opts: RollOptions = {}): DiceResult {
  const floor = opts.floor ?? 0;
  let floored = false;
  const terms = expr.terms.map((term): RolledTerm => {
    if (term.kind === "flat") return { term, rolls: [], value: term.value };
    const rolls = Array.from({ length: term.count }, () => {
      const raw = checkedRoll(die, term.sides);
      if (raw < floor) floored = true;
      return Math.max(raw, floor);
    });
    return { term, rolls, value: term.sign * rolls.reduce((a, b) => a + b, 0) };
  });
  const total = terms.reduce((a, t) => a + t.value, 0);
  const pieces = terms.map(({ term, rolls }, i) => {
    const sign = termSign(term) < 0 ? "-" : i > 0 ? "+" : "";
    const dice = term.kind === "dice" ? ` [${rolls.join(", ")}]` : "";
    return `${sign}${termText(term)}${dice}`;
  });
  const notes = [opts.label, floored ? `dice below ${floor} count as ${floor}` : undefined].filter(Boolean);
  const breakdown = `${pieces.join(" ")} = ${total}${notes.length ? `, ${notes.join(", ")}` : ""}`;
  return { notation: formatNotation(expr), total, terms, breakdown };
}

/** A d20 test: one d20, or two keeping the higher (advantage) or the lower (disadvantage), plus `modifier`. */
export function rollD20(modifier: number, mode: RollMode, die: DieRoller, opts: { critRange?: number } = {}): D20Result {
  const dice = mode === "normal" ? [checkedRoll(die, 20)] : [checkedRoll(die, 20), checkedRoll(die, 20)];
  const natural = mode === "disadvantage" ? Math.min(...dice) : Math.max(...dice);
  const total = natural + modifier;
  const isNat20 = natural === 20;
  const isNat1 = natural === 1;
  const notes = [mode === "normal" ? undefined : mode, isNat20 ? "natural 20" : undefined, isNat1 ? "natural 1" : undefined].filter(Boolean);
  const breakdown = `d20 [${dice.join(", ")}] ${signedText(modifier)} = ${total}${notes.length ? `, ${notes.join(", ")}` : ""}`;
  return { mode, dice, natural, modifier, total, isNat20, isNat1, isCrit: natural >= (opts.critRange ?? 20), breakdown };
}

/** Plays back fixed rolls (tests). Throws when it runs out. */
export function sequenceDie(values: readonly number[]): DieRoller {
  let i = 0;
  return () => {
    if (i >= values.length) throw new DiceError("The scripted die ran out of rolls.");
    return values[i++]!;
  };
}

/** A reproducible die (mulberry32), for tests and distribution checks. */
export function seededDie(seed: number): DieRoller {
  let a = seed >>> 0;
  return (sides) => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    const r = ((t ^ (t >>> 14)) >>> 0) / UINT32_RANGE;
    return Math.floor(r * sides) + 1;
  };
}

/** An unbiased die over a 32-bit random source: values in the uneven tail are drawn again (rejection sampling). */
export function makeCryptoDie(fill: (buf: Uint32Array) => void): DieRoller {
  const buf = new Uint32Array(1);
  return (sides) => {
    const limit = UINT32_RANGE - (UINT32_RANGE % sides);
    for (;;) {
      fill(buf);
      const v = buf[0]!;
      if (v < limit) return (v % sides) + 1;
    }
  };
}

/** The app's die: `crypto.getRandomValues`. */
export const cryptoDie: DieRoller = makeCryptoDie((buf) => { crypto.getRandomValues(buf); });
