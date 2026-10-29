import type { PlayConcentrationSpell, PlaySlotUse, PlayState, PlayTracker, PlayTrackerReset, SpellcastingState } from "../types";
import { cleanTrackerName } from "../../lib/trackerName";
import { apiError } from "./errors";

/** The mock's spell tracker state (GitHub #2), kept on `MockPlay.tracking`. */
export interface MockTracking {
  /** Used slots per spell level ("1".."9"). */
  slotsUsed: Record<string, number>;
  pactUsed: number;
  concentration: string | null;
  trackers: PlayTracker[];
}

export const emptyTracking = (): MockTracking => ({ slotsUsed: {}, pactUsed: 0, concentration: null, trackers: [] });

export const MAX_TRACKER_NAME = 60;
export const MAX_TRACKER_MAX = 999;
export const MAX_TRACKERS = 50;

type TrackerPlay = Pick<PlayState, "spellSlots" | "pactSlots" | "concentration" | "concentrationSpells" | "trackers">;

/** As the host: a known (picked) or prepared spell with concentration, by level and name. */
export function concentrationSpells(sc: SpellcastingState): PlayConcentrationSpell[] {
  const seen = new Map<string, PlayConcentrationSpell>();
  for (const s of sc.classes.flatMap((c) => c.spells)) {
    if (s.concentration && (s.slotId !== null || s.prepared || s.alwaysPrepared) && !seen.has(s.id)) seen.set(s.id, { id: s.id, name: s.name, level: s.level });
  }
  return [...seen.values()].sort((a, b) => a.level - b.level || a.name.localeCompare(b.name));
}

const pactOf = (sc: SpellcastingState) => sc.classes.find((c) => c.pact)?.pact ?? null;
const slotMax = (sc: SpellcastingState, level: number): number => sc.slots.find((r) => r.level === level)?.total ?? 0;

/** The tracker part of `CharacterSnapshot.play`, clamped to the current maximums. */
export function trackerPlay(t: MockTracking | undefined, sc: SpellcastingState): TrackerPlay {
  const tr = t ?? emptyTracking();
  const spells = concentrationSpells(sc);
  const pact = pactOf(sc);
  const spellSlots: PlaySlotUse[] = sc.slots.filter((r) => r.total > 0)
    .map((r) => ({ level: r.level, max: r.total, used: Math.min(r.total, tr.slotsUsed[String(r.level)] ?? 0) }));
  return {
    spellSlots,
    pactSlots: pact && pact.total > 0 ? { level: pact.level, max: pact.total, used: Math.min(pact.total, tr.pactUsed) } : null,
    concentration: spells.find((s) => s.id === tr.concentration) ?? null,
    concentrationSpells: spells,
    trackers: tr.trackers.map((x) => ({ ...x, used: Math.min(x.used, x.max) })),
  };
}

function slotPool(tr: MockTracking, sc: SpellcastingState, level: number, pact: boolean): { max: number; used: number; label: string } {
  const max = pact ? (pactOf(sc)?.total ?? 0) : slotMax(sc, level);
  const label = pact ? "Pact Magic slots" : `level ${level} spell slots`;
  if (max === 0) throw apiError("requirement_failed", `The character has no ${label}`);
  return { max, label, used: Math.min(max, pact ? tr.pactUsed : (tr.slotsUsed[String(level)] ?? 0)) };
}

const withSlots = (tr: MockTracking, level: number, pact: boolean, used: number): MockTracking =>
  (pact ? { ...tr, pactUsed: used } : { ...tr, slotsUsed: { ...tr.slotsUsed, [String(level)]: used } });

/** As the host: `n` more slots used, refused when fewer are left. */
export function spendSlots(t: MockTracking | undefined, sc: SpellcastingState, level: number, pact: boolean, n: number): MockTracking {
  const tr = t ?? emptyTracking();
  const { max, label, used } = slotPool(tr, sc, level, pact);
  if (n > max - used) throw apiError("requirement_failed", `${max - used} of ${max} ${label} left`);
  return withSlots(tr, level, pact, used + n);
}

/** As the host: `n` slots back, never below none used. */
export function restoreSlots(t: MockTracking | undefined, sc: SpellcastingState, level: number, pact: boolean, n: number): MockTracking {
  const tr = t ?? emptyTracking();
  const { used } = slotPool(tr, sc, level, pact);
  return withSlots(tr, level, pact, Math.max(0, used - n));
}

export function setConcentration(t: MockTracking | undefined, sc: SpellcastingState, spellId: string | null): MockTracking {
  if (spellId !== null && !concentrationSpells(sc).some((s) => s.id === spellId)) {
    throw apiError("requirement_failed", `${spellId} is not a concentration spell this character knows or has prepared`);
  }
  return { ...(t ?? emptyTracking()), concentration: spellId };
}


function checked(name: string, max: number): string {
  const n = cleanTrackerName(name);
  if (!n) throw apiError("invalid_argument", "Give the tracker a name");
  if ([...n].length > MAX_TRACKER_NAME) throw apiError("invalid_argument", `A tracker name has at most ${MAX_TRACKER_NAME} characters`);
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f]/.test(n)) throw apiError("invalid_argument", "A tracker name cannot hold control characters");
  if (!Number.isInteger(max) || max < 1 || max > MAX_TRACKER_MAX) throw apiError("invalid_argument", `A tracker maximum is between 1 and ${MAX_TRACKER_MAX}`);
  return n;
}

/** As the host: one above the highest number in use, or the lowest free number when that would overflow. */
function nextId(ts: PlayTracker[]): string {
  const used = new Set(ts.map((x) => Number(/^tracker-(\d+)$/.exec(x.id)?.[1] ?? 0)));
  const top = Math.max(0, ...used);
  if (top < 2 ** 32 - 1) return `tracker-${top + 1}`;
  let n = 1;
  while (used.has(n)) n += 1;
  return `tracker-${n}`;
}

function find(tr: MockTracking, id: string): PlayTracker {
  const x = tr.trackers.find((y) => y.id === id);
  if (!x) throw apiError("not_found", `Unknown element ${id}`);
  return x;
}

export function addTracker(t: MockTracking | undefined, name: string, max: number, reset: PlayTrackerReset, countsUp = false): MockTracking {
  const tr = t ?? emptyTracking();
  const n = checked(name, max);
  if (tr.trackers.length >= MAX_TRACKERS) throw apiError("invalid_argument", `A character has at most ${MAX_TRACKERS} trackers`);
  return { ...tr, trackers: [...tr.trackers, { id: nextId(tr.trackers), name: n, max, used: 0, reset, countsUp }] };
}

export function updateTracker(t: MockTracking | undefined, id: string, name: string, max: number, reset: PlayTrackerReset, countsUp = false): MockTracking {
  const tr = t ?? emptyTracking();
  const old = find(tr, id);
  const n = checked(name, max);
  return { ...tr, trackers: tr.trackers.map((x) => (x.id === id ? { ...old, name: n, max, reset, countsUp, used: Math.min(old.used, max) } : x)) };
}

export function removeTracker(t: MockTracking | undefined, id: string): MockTracking {
  const tr = t ?? emptyTracking();
  find(tr, id);
  return { ...tr, trackers: tr.trackers.filter((x) => x.id !== id) };
}

export function spendTracker(t: MockTracking | undefined, id: string, n: number): MockTracking {
  const tr = t ?? emptyTracking();
  const x = find(tr, id);
  const used = Math.min(x.used, x.max);
  if (n > x.max - used) throw apiError("requirement_failed", `${x.name} has ${x.max - used} of ${x.max} pips left`);
  return { ...tr, trackers: tr.trackers.map((y) => (y.id === id ? { ...y, used: used + n } : y)) };
}

export function restoreTracker(t: MockTracking | undefined, id: string, n: number): MockTracking {
  const tr = t ?? emptyTracking();
  const x = find(tr, id);
  return { ...tr, trackers: tr.trackers.map((y) => (y.id === id ? { ...y, used: Math.max(0, Math.min(x.used, x.max) - n) } : y)) };
}

/** As the host's rests: a short rest gives back Pact Magic slots and short-rest trackers; a long rest every slot and short- or long-rest tracker, and ends concentration. */
export function afterRest(t: MockTracking | undefined, rest: "short" | "long"): MockTracking {
  const tr = t ?? emptyTracking();
  const long = rest === "long";
  const resets = (r: PlayTrackerReset) => r === "short_rest" || (long && r === "long_rest");
  return {
    slotsUsed: long ? {} : tr.slotsUsed,
    pactUsed: 0,
    concentration: long ? null : tr.concentration,
    trackers: tr.trackers.map((x) => (resets(x.reset) ? { ...x, used: 0 } : x)),
  };
}
