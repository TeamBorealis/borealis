import type { CharacterSnapshot } from "../../api/contract";
import { openPackages } from "../../pages/equipment/startingPackages";

/** The creation tabs (Build, Magic, Gear): where Next selection leads, and where the finished character is offered. */
const CREATION_PATHS = ["/builder", "/spells", "/equipment"] as const;

/** Route that opens the sheet with its preview generated (on a phone: the full-screen preview, TV2 C11/C16). */
export const VIEW_SHEET_PATH = "/sheet?preview=1";

/** True on Build, Magic or Gear (and their sub-routes). */
export function onCreationTab(pathname: string): boolean {
  return CREATION_PATHS.some((p) => pathname === p || pathname.startsWith(`${p}/`));
}

/**
 * True when the character has no open pick: no open selection and, since tester B3 (B-129), no untaken starting-equipment
 * package (it counts as open until taken or declined with "I'll buy my own").
 */
export function hasNoOpenPick(snapshot: CharacterSnapshot | null): boolean {
  if (!snapshot || snapshot.progress.nextSlotId !== null || snapshot.progress.remaining > 0) return false;
  return openPackages(snapshot).length === 0;
}

/** What the status line knows about the open character's complete moment. */
export interface CompleteMoment {
  /** The character it tracks; another character starts over. */
  characterId: string | null;
  /** The character had no open pick at the last look. */
  complete: boolean;
  /** View sheet and Play are on offer. */
  shown: boolean;
}

export const NO_MOMENT: CompleteMoment = { characterId: null, complete: false, shown: false };

/**
 * U23 (B-143, refines TV1 C10 option a): creation's complete moment is a moment, not a fixture. On a phone it appears on
 * Build, Magic or Gear when the session's last open pick is made (open picks go from some to none while the character is
 * open), and goes when the user leaves those tabs (View sheet and Play both leave them) or dismisses it. A character opened
 * already complete never gets it: the bottom tabs already reach Sheet and Play. Picks that open again (a level up) and are
 * made bring it back.
 */
export function stepCompleteMoment(prev: CompleteMoment, snapshot: CharacterSnapshot | null, pathname: string): CompleteMoment {
  const characterId = snapshot?.id ?? null;
  const complete = hasNoOpenPick(snapshot);
  if (characterId !== prev.characterId) return { characterId, complete, shown: false };
  const shown = complete && onCreationTab(pathname) && (prev.shown || !prev.complete);
  return { characterId, complete, shown };
}

export const sameMoment = (a: CompleteMoment, b: CompleteMoment): boolean =>
  a.characterId === b.characterId && a.complete === b.complete && a.shown === b.shown;

/** The moment once the user dismissed it (or took one of its offers): it stays away until picks open and close again. */
export const dismissMoment = (m: CompleteMoment): CompleteMoment => ({ ...m, shown: false });
