import type { CharacterSnapshot } from "../../api/contract";

/**
 * Open picks a bottom tab's badge counts. Tester B13 (B-136): on a phone each tab counts the picks made on it, so Magic
 * shows its spell picks and Build only the picks on its own tabs. A desktop window keeps one count on Build.
 */
export function openPicksFor(tabId: string, snapshot: CharacterSnapshot | null, phone: boolean): number {
  if (!snapshot) return 0;
  const { remaining, byTab } = snapshot.progress;
  if (!phone) return tabId === "build" ? remaining : 0;
  const spells = byTab.spells ?? 0;
  if (tabId === "build") return Math.max(0, remaining - spells);
  if (tabId === "magic") return spells;
  return 0;
}
