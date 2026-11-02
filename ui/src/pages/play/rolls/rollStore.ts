import { useSyncExternalStore } from "react";
import { type DieRoller, cryptoDie } from "../../../lib/dice";

/**
 * Recent rolls for this app session (GitHub #1): kept in memory per character, newest first, never saved with the
 * character and gone when the app restarts. The Play tab's result card shows the newest one.
 */
export interface RollEntry {
  id: number;
  characterId: string;
  /** "Stealth check", "Longsword damage". */
  label: string;
  total: number;
  /** "d20 [14, 7] +5 = 19, advantage". */
  breakdown: string;
  /** Epoch milliseconds. */
  at: number;
  /** A d20 attack roll that can be followed by its damage roll (doubled on a critical hit). */
  followUp?: { label: string; critical: boolean } | null;
}

export const MAX_ROLLS = 50;

let entries: readonly RollEntry[] = [];
let seq = 0;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

export function addRoll(entry: Omit<RollEntry, "id" | "at">): RollEntry {
  const full: RollEntry = { ...entry, id: ++seq, at: Date.now() };
  entries = [full, ...entries].slice(0, MAX_ROLLS);
  emit();
  return full;
}

export function clearRolls(characterId: string): void {
  entries = entries.filter((e) => e.characterId !== characterId);
  emit();
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => { listeners.delete(l); };
};
const snapshot = () => entries;

/** The session's rolls of one character, newest first. */
export function useRolls(characterId: string): RollEntry[] {
  const all = useSyncExternalStore(subscribe, snapshot, snapshot);
  return all.filter((e) => e.characterId === characterId);
}

let die: DieRoller = cryptoDie;
/** The die every Play roll uses: `crypto.getRandomValues` in the app. */
export const currentDie = (): DieRoller => die;

/** Tests script the dice and start from an empty history. */
export function setDieForTests(next: DieRoller | null): void {
  die = next ?? cryptoDie;
  entries = [];
  emit();
}
