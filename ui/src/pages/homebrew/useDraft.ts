import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import type { HomebrewDraftView, HomebrewForm, HomebrewProblem, HomebrewSaveResult } from "../../api/contract";
import { canRedo, canUndo } from "./history";
import { type AutosaveState, type DraftApi, DraftSession, type DraftSessionOptions, mergeProblems } from "./draftSession";

export interface DraftController {
  /** The form on screen (null while no element is open). */
  form: HomebrewForm | null;
  /** The last view the backend sent: draft id, preview, generated data, `baseChanged`. */
  view: HomebrewDraftView | null;
  /** Live problems, then the engine checks' (spec 8.1). */
  problems: HomebrewProblem[];
  autosave: AutosaveState;
  error: string | null;
  checking: boolean;
  saving: boolean;
  canUndo: boolean;
  canRedo: boolean;
  setField: (key: string, value: unknown, typing?: boolean) => void;
  edit: (next: HomebrewForm | ((form: HomebrewForm) => HomebrewForm), group?: string | null) => void;
  undo: () => void;
  redo: () => void;
  open: (view: HomebrewDraftView | null) => Promise<void>;
  rebase: (view: HomebrewDraftView) => void;
  flush: () => Promise<boolean>;
  save: () => Promise<HomebrewSaveResult>;
  check: () => Promise<void>;
}

/**
 * The creator's draft state (spec 15.3 `useDraft`): debounced autosave, undo/redo, engine checks. Leaving the page sends
 * what is pending (spec 10.1: closing asks nothing and loses nothing).
 */
export function useDraft(api: DraftApi, opts?: DraftSessionOptions): DraftController {
  const [session] = useState(() => new DraftSession(api, opts));
  const snap = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  useEffect(() => () => session.dispose(), [session]);
  return useMemo(() => {
    const h = snap.history;
    return {
      form: h?.present ?? null,
      view: snap.view,
      problems: mergeProblems(snap.view?.problems ?? [], snap.engineProblems),
      autosave: snap.autosave,
      error: snap.error,
      checking: snap.checking,
      saving: snap.saving,
      canUndo: !!h && canUndo(h),
      canRedo: !!h && canRedo(h),
      setField: (key, value, typing) => session.setField(key, value, typing),
      edit: (next, group) => session.edit(next, group ?? null),
      undo: () => session.undo(),
      redo: () => session.redo(),
      open: (view) => session.open(view),
      rebase: (view) => session.rebase(view),
      flush: () => session.flush(),
      save: () => session.save(),
      check: () => session.check(),
    };
  }, [snap, session]);
}
