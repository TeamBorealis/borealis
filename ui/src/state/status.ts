import { useSyncExternalStore } from "react";
import type { BuilderTab, CharacterSnapshot, SelectionSlot } from "../api/contract";
import { isPhone } from "../lib/phone";
import { openPackages } from "../pages/equipment/startingPackages";

/** Status-bar message store: the bar logs what just happened (the classic builder keeps the last action visible until the next one). */
let message = "";
const listeners = new Set<() => void>();

export function setStatus(next: string): void {
  if (next === message) return;
  message = next;
  listeners.forEach((l) => l());
}
export const getStatus = (): string => message;
export function useStatusMessage(): string {
  return useSyncExternalStore((cb) => { listeners.add(cb); return () => { listeners.delete(cb); }; }, getStatus, getStatus);
}


function inventoryTotals(s: CharacterSnapshot): Map<string, { name: string; qty: number }> {
  const totals = new Map<string, { name: string; qty: number }>();
  for (const it of s.inventory.items) {
    const cur = totals.get(it.itemId);
    totals.set(it.itemId, { name: it.displayName || it.name, qty: (cur?.qty ?? 0) + it.quantity });
  }
  return totals;
}

/** Tester B3 (B-129): on a phone an untaken starting-equipment package is the last open choice. */
const STARTING_EQUIPMENT_PROMPT = "Next: choose your starting equipment (Gear tab)";

/** The Build section names (BuilderPage's tab labels); `race` follows the ruleset's word. */
const SECTION: Record<Exclude<BuilderTab, "race" | "spells">, string> = {
  class: "Class", background: "Background", abilities: "Ability Scores", feats: "Feats", languages: "Languages",
  proficiencies: "Proficiencies", companion: "Companion",
};

/** Where a slot is picked, in the phone's words: the bottom tab, and the Build section. */
function placeOf(slot: SelectionSlot, s: CharacterSnapshot): string {
  if (slot.tab === "spells") return "Magic tab";
  const section = slot.tab === "race" ? (s.ruleset === "dnd2024" ? "Species" : "Race") : SECTION[slot.tab];
  return `Build tab, ${section}`;
}

/**
 * Message prompting for the next pending selection ("Choose one from the 'Race' selection option."), or the completion line.
 * Tester B7 (B-133, TV2): on a phone it reads as a to-do and names where the pick is made
 * ("Next: choose Magic Initiate Cantrips, 1 of 2 (Magic tab)"); desktop keeps Aurora's text (U7).
 */
export function nextSelectionPrompt(s: CharacterSnapshot): string {
  const next = s.slots.find((x) => x.id === s.progress.nextSlotId);
  if (!next) return isPhone() && openPackages(s).length > 0 ? STARTING_EQUIPMENT_PROMPT : "All required selections are made.";
  if (!isPhone()) return `Choose one from the '${next.title}' selection option.`;
  const count = next.of > 1 ? `, ${next.ordinal} of ${next.of}` : "";
  return `Next: choose ${next.title}${count} (${placeOf(next, s)})`;
}

/** Status line for a freshly adopted snapshot (loaded from disk, or just created). */
export function describeAdopt(s: CharacterSnapshot): string {
  const untouched = s.progress.remaining > 0 && s.slots.every((x) => x.locked || !x.chosen);
  return untouched ? nextSelectionPrompt(s) : `Loaded '${s.name}'`;
}

/** Status line describing what a mutation changed, or null when nothing worth logging changed. */
export function describeChange(prev: CharacterSnapshot, next: CharacterSnapshot): string | null {
  const before = inventoryTotals(prev);
  const after = inventoryTotals(next);
  for (const [id, a] of after) {
    const delta = a.qty - (before.get(id)?.qty ?? 0);
    if (delta > 0) return `You have added ${delta} x '${a.name}' to your inventory.`;
  }
  for (const [id, b] of before) {
    const delta = b.qty - (after.get(id)?.qty ?? 0);
    if (delta > 0) return `You have removed ${delta} x '${b.name}' from your inventory.`;
  }
  if (next.level.total !== prev.level.total) return `Your character is now level ${next.level.total}.`;
  const pickedSlot = next.slots.find((s) => s.chosen && !s.locked && prev.slots.find((p) => p.id === s.id)?.chosen?.id !== s.chosen.id);
  if (pickedSlot) return nextSelectionPrompt(next);
  // A10: the prompt names the slot Next selection goes to, so it follows that slot when it changes without a pick (a clear).
  if (next.progress.nextSlotId !== prev.progress.nextSlotId) return nextSelectionPrompt(next);
  // B3: declining a package ("I'll buy my own") changes no inventory, so the prompt moves on here.
  if (isPhone() && openPackages(next).length !== openPackages(prev).length) return nextSelectionPrompt(next);
  return null;
}
