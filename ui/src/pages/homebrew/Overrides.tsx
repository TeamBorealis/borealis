import { useState } from "react";
import type { ElementChangedBy, HomebrewOverrideInfo } from "../../api/contract";
import { useToast } from "../../components";
import { useApi } from "../../state/ApiContext";
import { usePlatform } from "../../state/platform";
import { errorMessage } from "../../state/useAsync";

/** HC20 (spec 22.6): what the creator says when an override's original changed underneath or is not loaded. */
export const ORIGINAL_CHANGED = "The original changed after you changed it (its book was imported again). Save to keep your version, or Revert to use the new original.";
export const ORIGINAL_MISSING = "The original isn't loaded (its book is off or removed), so this change has no effect until it is.";

/** The element list's note on an override row (spec 22.4). */
export function overrideNote(info: HomebrewOverrideInfo): string {
  if (info.missing) return "Original not loaded";
  if (info.changed) return "Original changed";
  return info.sourceName ? `Changes ${info.sourceName}` : "Changes the original";
}

/** Revert an override (spec 22.6): the original returns; the toast offers Undo. */
function useRevert(packId: string, elementId: string, onDone: () => void) {
  const api = useApi();
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const revert = async () => {
    setBusy(true);
    try {
      const r = await api.homebrewRevertOverride({ packId, elementId });
      const undoId = r.undoId;
      toast.push("Reverted. The original is back.", "info", undoId ? {
        label: "Undo",
        run: () => { void api.homebrewUndo({ undoId }).then(onDone, (e: unknown) => toast.push(errorMessage(e), "error")); },
      } : undefined);
      onDone();
    } catch (e) {
      toast.push(errorMessage(e), "error");
    } finally {
      setBusy(false);
    }
  };
  return { busy, revert };
}

/**
 * "Changed by <pack>" (spec 22.6) in a description pane or the Compendium: a line in the pane's own style, with Revert
 * while the creator is on.
 */
export function ChangedByLine({ elementId, changedBy, onReverted }: { elementId: string; changedBy: ElementChangedBy; onReverted: () => void }) {
  const { creatorEnabled } = usePlatform();
  const { busy, revert } = useRevert(changedBy.packId, elementId, onReverted);
  return (
    <p className="desc-line desc-changed" role="note" aria-label="Changed by">
      <i>Changed by.</i> {changedBy.packName}
      {creatorEnabled && (
        <>
          {" "}
          <button type="button" className="desc-source desc-revert" disabled={busy} aria-label={`Revert to the original (remove the change by ${changedBy.packName})`}
            onClick={() => void revert()}>Revert</button>
        </>
      )}
    </p>
  );
}

/**
 * The creator's override notice (spec 22.4, 22.6) above the form: what saving does, and when the original changed
 * underneath or is not loaded; Revert once the override is saved.
 */
export function OverrideNotice({ info, packId, saved, onReverted }: { info: HomebrewOverrideInfo; packId: string; saved: boolean; onReverted: () => void }) {
  const { busy, revert } = useRevert(packId, info.originalId, onReverted);
  const book = info.sourceName || "its book";
  return (
    <div className={`hb-override${info.changed || info.missing ? " hb-override-warn" : ""}`} role="note" aria-label="Override">
      <p>Override of an element of {book}. Saving changes it everywhere; characters that use it get the change.</p>
      {info.changed && <p className="hb-override-alert" role="alert">{ORIGINAL_CHANGED}</p>}
      {info.missing && <p className="hb-override-alert" role="alert">{ORIGINAL_MISSING}</p>}
      {saved && <button type="button" className="hb-tool" disabled={busy} onClick={() => void revert()}>Revert</button>}
    </div>
  );
}
