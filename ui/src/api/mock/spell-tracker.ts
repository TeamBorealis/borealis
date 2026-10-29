import type { PlayConcentrationSpell, PlaySlotUse, PlayState, PlayTracker, PlayTrackerReset, SpellcastingState } from "../types";
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

export function setSlotsUsed(t: MockTracking | undefined, sc: SpellcastingState, level: number, used: number, pact: boolean): MockTracking {
  const tr = t ?? emptyTracking();
  const max = pact ? (pactOf(sc)?.total ?? 0) : slotMax(sc, level);
  const label = pact ? "Pact Magic slots" : `level ${level} spell slots`;
  if (max === 0) throw apiError("requirement_failed", `The character has no ${label}`);
  if (used < 0 || used > max) throw apiError("requirement_failed", `The character has ${max} ${label}`);
  return pact ? { ...tr, pactUsed: used } : { ...tr, slotsUsed: { ...tr.slotsUsed, [String(level)]: used } };
}

export function setConcentration(t: MockTracking | undefined, sc: SpellcastingState, spellId: string | null): MockTracking {
  if (spellId !== null && !concentrationSpells(sc).some((s) => s.id === spellId)) {
    throw apiError("requirement_failed", `${spellId} is not a concentration spell this character knows or has prepared`);
  }
  return { ...(t ?? emptyTracking()), concentration: spellId };
}

function checked(name: string, max: number): string {
  const n = name.trim();
  if (!n) throw apiError("invalid_argument", "Give the tracker a name");
  if ([...n].length > MAX_TRACKER_NAME) throw apiError("invalid_argument", `A tracker name has at most ${MAX_TRACKER_NAME} characters`);
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f]/.test(n)) throw apiError("invalid_argument", "A tracker name cannot hold control characters");
  if (!Number.isInteger(max) || max < 1 || max > MAX_TRACKER_MAX) throw apiError("invalid_argument", `A tracker maximum is between 1 and ${MAX_TRACKER_MAX}`);
  return n;
}

const nextId = (ts: PlayTracker[]): string => `tracker-${1 + Math.max(0, ...ts.map((x) => Number(/^tracker-(\d+)$/.exec(x.id)?.[1] ?? 0)))}`;

function find(tr: MockTracking, id: string): PlayTracker {
  const x = tr.trackers.find((y) => y.id === id);
  if (!x) throw apiError("not_found", `Unknown element ${id}`);
  return x;
}

export function addTracker(t: MockTracking | undefined, name: string, max: number, reset: PlayTrackerReset): MockTracking {
  const tr = t ?? emptyTracking();
  const n = checked(name, max);
  if (tr.trackers.length >= MAX_TRACKERS) throw apiError("invalid_argument", `A character has at most ${MAX_TRACKERS} trackers`);
  return { ...tr, trackers: [...tr.trackers, { id: nextId(tr.trackers), name: n, max, used: 0, reset }] };
}

export function updateTracker(t: MockTracking | undefined, id: string, name: string, max: number, reset: PlayTrackerReset): MockTracking {
  const tr = t ?? emptyTracking();
  const old = find(tr, id);
  const n = checked(name, max);
  return { ...tr, trackers: tr.trackers.map((x) => (x.id === id ? { ...old, name: n, max, reset, used: Math.min(old.used, max) } : x)) };
}

export function removeTracker(t: MockTracking | undefined, id: string): MockTracking {
  const tr = t ?? emptyTracking();
  find(tr, id);
  return { ...tr, trackers: tr.trackers.filter((x) => x.id !== id) };
}

export function setTrackerUsed(t: MockTracking | undefined, id: string, used: number): MockTracking {
  const tr = t ?? emptyTracking();
  const x = find(tr, id);
  if (!Number.isInteger(used) || used < 0 || used > x.max) throw apiError("invalid_argument", `${x.name} has ${x.max} pips`);
  return { ...tr, trackers: tr.trackers.map((y) => (y.id === id ? { ...y, used } : y)) };
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
