import { useSyncExternalStore } from "react";
import type { CharacterSnapshot } from "../api/contract";

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

/** Message prompting for the next pending selection ("Choose one from the 'Race' selection option."), or the completion line. */
export function nextSelectionPrompt(s: CharacterSnapshot): string {
  const next = s.slots.find((x) => x.id === s.progress.nextSlotId);
  return next ? `Choose one from the '${next.title}' selection option.` : "All required selections are made.";
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
  return null;
}
