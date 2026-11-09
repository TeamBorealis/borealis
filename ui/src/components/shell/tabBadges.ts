import type { CharacterSnapshot } from "../../api/contract";
import { openPackages } from "../../pages/equipment/startingPackages";

/**
 * Open picks a bottom tab's badge counts. Tester B13 (B-136): on a phone each tab counts the picks made on it, so Magic
 * shows its spell picks and Build only the picks on its own tabs. Tester B3 (B-129): Gear counts the starting-equipment
 * packages not yet taken or declined. A desktop window keeps one count on Build.
 */
export function openPicksFor(tabId: string, snapshot: CharacterSnapshot | null, phone: boolean): number {
  if (!snapshot) return 0;
  const { remaining, byTab } = snapshot.progress;
  if (!phone) return tabId === "build" ? remaining : 0;
  const spells = byTab.spells ?? 0;
  if (tabId === "build") return Math.max(0, remaining - spells);
  if (tabId === "magic") return spells;
  if (tabId === "equipment") return openPackages(snapshot).length;
  return 0;
}
