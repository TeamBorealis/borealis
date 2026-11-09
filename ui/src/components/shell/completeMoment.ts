import type { CharacterSnapshot } from "../../api/contract";

/** The creation tabs (Build, Magic, Gear): where Next selection leads, and where the finished character is offered. */
const CREATION_PATHS = ["/builder", "/spells", "/equipment"] as const;

/** Route that opens the sheet with its preview generated (on a phone: the full-screen preview, TV2 C11/C16). */
export const VIEW_SHEET_PATH = "/sheet?preview=1";

/**
 * TV1 C10 (B-143, option a): creation gets a "complete" moment. On a phone, when no selection is open, the status bar on
 * the creation tabs offers View sheet and Play where Next selection stood. Play and the sheet themselves don't offer it.
 */
export function offersCompleteMoment(snapshot: CharacterSnapshot | null, pathname: string): boolean {
  if (!snapshot || snapshot.progress.nextSlotId !== null || snapshot.progress.remaining > 0) return false;
  return CREATION_PATHS.some((p) => pathname === p || pathname.startsWith(`${p}/`));
}
