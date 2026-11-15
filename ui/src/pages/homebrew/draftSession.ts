import type { Api, HomebrewDraftView, HomebrewForm, HomebrewProblem, HomebrewSaveResult } from "../../api/contract";
import { isApiError } from "../../api/contract";
import { errorMessage } from "../../state/useAsync";
import { type History, canRedo, canUndo, record, redo, startHistory, undo, TYPING_PAUSE_MS, HISTORY_LIMIT } from "./history";

/**
 * One open draft of the homebrew creator (spec 10.1-10.2, 8.1): the undo history of its form, the debounced autosave
 * (`homebrew_update_draft`) and the paused engine check (`homebrew_check_draft`).
 *
 * Rules:
 * - The form on screen is the source of truth while the draft is open. Server answers bring problems, the live preview and
 *   the generated data; they never overwrite what the user typed (a trimmed name would eat the space just typed).
 * - One update is in flight at a time. Edits made meanwhile are sent, as the latest form, once it answers.
 * - An answer for a draft that is no longer open is dropped.
 * - Closing, switching drafts or leaving the page sends what is pending: nothing is lost and nothing is asked.
 */
export type AutosaveState = "saved" | "pending" | "saving" | "error";

export type DraftApi = Pick<Api, "homebrewUpdateDraft" | "homebrewCheckDraft" | "homebrewSaveDraft">;

export interface DraftSessionOptions {
  /** Autosave debounce after the last edit (spec 8.1: 300 ms). */
  debounceMs?: number;
  /** Pause before the engine checks run (spec 8.1 step 6: 1.5 s). */
  checkPauseMs?: number;
  typingPauseMs?: number;
  historyLimit?: number;
  now?: () => number;
}

export interface DraftSnapshot {
  /** The last view the backend sent for this draft (its id, problems, preview, generated data). */
  readonly view: HomebrewDraftView | null;
  readonly history: History<HomebrewForm> | null;
  readonly autosave: AutosaveState;
  /** The last autosave or save error, in the backend's words. */
  readonly error: string | null;
  /** Problems of the engine checks and the level sweep (step 6) for the last autosaved form. A newer autosave makes them
   * stale, so they go with its answer and come back from the next check (1.5 s later) if they still hold. */
  readonly engineProblems: readonly HomebrewProblem[];
  readonly checking: boolean;
  readonly saving: boolean;
}

export const AUTOSAVE_DEBOUNCE_MS = 300;
export const CHECK_PAUSE_MS = 1500;

const EMPTY: DraftSnapshot = { view: null, history: null, autosave: "saved", error: null, engineProblems: [], checking: false, saving: false };

/** Two forms hold the same values (the backend's copy of a form is equal, not the same object). */
export function sameForm(a: HomebrewForm | null, b: HomebrewForm | null): boolean {
  return a === b || (a !== null && b !== null && JSON.stringify(a) === JSON.stringify(b));
}

/** The draft's problems: the live pipeline's, then the engine checks' that are not already listed. */
export function mergeProblems(live: readonly HomebrewProblem[], engine: readonly HomebrewProblem[]): HomebrewProblem[] {
  const key = (p: HomebrewProblem) => `${p.severity}|${p.code}|${p.path ?? ""}|${p.elementId ?? ""}|${p.message}`;
  const seen = new Set(live.map(key));
  return [...live, ...engine.filter((p) => !seen.has(key(p)))];
}

/** The backend has no engine check yet (`details.notImplemented`): the panel simply shows no engine problems. */
const notImplemented = (e: unknown): boolean => isApiError(e) && typeof e.details === "object" && e.details !== null && "notImplemented" in e.details;

export class DraftSession {
  private snap: DraftSnapshot = EMPTY;
  private readonly listeners = new Set<() => void>();
  /** The form the backend last acknowledged for this draft. */
  private acked: HomebrewForm | null = null;
  private inFlight: Promise<boolean> | null = null;
  private saveTimer: ReturnType<typeof setTimeout> | null = null;
  private checkTimer: ReturnType<typeof setTimeout> | null = null;
  /** Bumped whenever another draft is opened: older answers are dropped. */
  private token = 0;
  /** The draft was saved (and so deleted on the backend): edits wait for `rebase` before they are sent. */
  private detached = false;
  /** The form a Save wrote (what was on screen when it was autosaved and saved); null when nothing was saved since. */
  private savedForm: HomebrewForm | null = null;
  private readonly opts: Required<DraftSessionOptions>;

  constructor(private readonly api: DraftApi, opts: DraftSessionOptions = {}) {
    this.opts = {
      debounceMs: opts.debounceMs ?? AUTOSAVE_DEBOUNCE_MS, checkPauseMs: opts.checkPauseMs ?? CHECK_PAUSE_MS,
      typingPauseMs: opts.typingPauseMs ?? TYPING_PAUSE_MS, historyLimit: opts.historyLimit ?? HISTORY_LIMIT, now: opts.now ?? Date.now,
    };
  }

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };

  readonly getSnapshot = (): DraftSnapshot => this.snap;

  get form(): HomebrewForm | null { return this.snap.history?.present ?? null; }

  get draftId(): string | null { return this.snap.view?.draftId ?? null; }

  /** Opens another draft with a fresh history. What the previous draft had pending is sent first. */
  async open(view: HomebrewDraftView | null): Promise<void> {
    await this.flush();
    this.clearTimers();
    this.token += 1;
    this.detached = false;
    this.savedForm = null;
    this.acked = view?.form ?? null;
    this.set({ ...EMPTY, view, history: view ? startHistory(view.form, this.opts.now()) : null });
  }

  /**
   * Continues with a new draft of the same element (after Save, the backend deletes the saved draft and the element is
   * opened again): the history stays, so Undo still steps back over the save. Unless it was edited since the save, the
   * form on screen becomes the element as the backend reads it back (its order and normalised values: the same element),
   * so nothing is autosaved and no draft of the just-saved element is left.
   */
  rebase(view: HomebrewDraftView): void {
    this.token += 1;
    this.detached = false;
    this.acked = view.form;
    const before = this.snap.history;
    const unedited = before !== null && this.savedForm !== null && sameForm(before.present, this.savedForm);
    this.savedForm = null;
    const history = before === null ? startHistory(view.form, this.opts.now()) : unedited ? { ...before, present: view.form, group: null } : before;
    this.set({ ...this.snap, view, history, error: null, engineProblems: [], autosave: sameForm(history.present, view.form) ? "saved" : "pending" });
    if (this.dirty()) this.schedule();
    else this.scheduleCheck();
  }

  /** Records an edit. `group` merges typing in one field into one undo step (spec 10.2). */
  edit(next: HomebrewForm | ((form: HomebrewForm) => HomebrewForm), group: string | null = null): void {
    const h = this.snap.history;
    if (!h) return;
    const form = typeof next === "function" ? next(h.present) : next;
    const history = record(h, form, { group, now: this.opts.now(), limit: this.opts.historyLimit, pauseMs: this.opts.typingPauseMs });
    if (history === h) return;
    this.changed(history);
  }

  /** Sets one field; `typing` groups keystrokes in that field into one undo step. */
  setField(key: string, value: unknown, typing = false): void {
    this.edit((f) => ({ ...f, fields: { ...f.fields, [key]: value } }), typing ? `field:${key}` : null);
  }

  undo(): void {
    const h = this.snap.history;
    if (h && canUndo(h)) this.changed(undo(h));
  }

  redo(): void {
    const h = this.snap.history;
    if (h && canRedo(h)) this.changed(redo(h));
  }

  /** Sends what is pending and waits for the backend. True when everything on screen is autosaved. */
  async flush(): Promise<boolean> {
    this.clearSaveTimer();
    for (;;) {
      if (this.inFlight) { await this.inFlight; continue; }
      if (!this.dirty() || this.detached) return !this.dirty();
      if (!(await this.send())) return false;
    }
  }

  /**
   * Save (Ctrl+S): autosaves what is pending, then validates and writes the pack. Rejects with the backend's error (problems
   * in red block it). After a save the backend has no draft: call `rebase` with the reopened element, or `open`.
   */
  async save(): Promise<HomebrewSaveResult> {
    const draftId = this.draftId;
    if (!draftId) throw new Error("No element is open.");
    this.set({ ...this.snap, saving: true });
    try {
      if (!(await this.flush())) throw new Error(this.snap.error ?? "The draft could not be autosaved.");
      const saved = this.form;
      const result = await this.api.homebrewSaveDraft({ draftId });
      this.savedForm = saved;
      this.detached = true;
      this.clearTimers();
      this.set({ ...this.snap, saving: false, error: null });
      return result;
    } catch (e) {
      this.set({ ...this.snap, saving: false, error: errorMessage(e) });
      throw e;
    }
  }

  /** Runs the engine checks now (opening PROBLEMS does this, spec 8.1 step 6). */
  async check(): Promise<void> {
    this.clearCheckTimer();
    const draftId = this.draftId;
    if (!draftId || this.dirty() || this.detached) return;
    const token = this.token;
    const form = this.form;
    this.set({ ...this.snap, checking: true });
    try {
      const problems = await this.api.homebrewCheckDraft({ draftId });
      // An edit while checking makes the answer stale: the next pause checks again.
      if (token === this.token && this.form === form) this.set({ ...this.snap, engineProblems: problems, checking: false });
      else if (token === this.token) this.set({ ...this.snap, checking: false });
    } catch (e) {
      if (token !== this.token) return;
      this.set({ ...this.snap, checking: false, engineProblems: notImplemented(e) ? [] : this.snap.engineProblems });
    }
  }

  /** Leaving: sends what is pending at once and stops the timers. The session stays usable. */
  dispose(): void {
    this.clearCheckTimer();
    if (this.saveTimer !== null || this.dirty()) void this.flush();
  }

  private dirty(): boolean {
    return this.snap.view !== null && !sameForm(this.form, this.acked);
  }

  private changed(history: History<HomebrewForm>): void {
    this.clearCheckTimer();
    this.set({ ...this.snap, history, autosave: "pending" });
    if (!this.dirty()) {
      this.clearSaveTimer();
      this.set({ ...this.snap, autosave: this.inFlight ? "saving" : "saved" });
      return;
    }
    if (!this.detached) this.schedule();
  }

  private schedule(): void {
    this.clearSaveTimer();
    this.saveTimer = setTimeout(() => { this.saveTimer = null; void this.flush(); }, this.opts.debounceMs);
  }

  private send(): Promise<boolean> {
    const view = this.snap.view;
    const form = this.form;
    if (!view || !form) return Promise.resolve(true);
    const token = this.token;
    this.set({ ...this.snap, autosave: "saving" });
    const flight = this.api.homebrewUpdateDraft({ draftId: view.draftId, form }).then(
      (answer) => {
        if (token !== this.token) return true;
        this.acked = form;
        const dirty = this.dirty();
        this.set({ ...this.snap, view: answer, error: null, engineProblems: [], autosave: dirty ? "pending" : "saved" });
        if (dirty) this.schedule();
        else this.scheduleCheck();
        return true;
      },
      (e: unknown) => {
        if (token === this.token) this.set({ ...this.snap, autosave: "error", error: errorMessage(e) });
        return false;
      },
    ).finally(() => { if (this.inFlight === flight) this.inFlight = null; });
    this.inFlight = flight;
    return flight;
  }

  private scheduleCheck(): void {
    this.clearCheckTimer();
    this.checkTimer = setTimeout(() => { this.checkTimer = null; void this.check(); }, this.opts.checkPauseMs);
  }

  private clearSaveTimer(): void {
    if (this.saveTimer !== null) clearTimeout(this.saveTimer);
    this.saveTimer = null;
  }

  private clearCheckTimer(): void {
    if (this.checkTimer !== null) clearTimeout(this.checkTimer);
    this.checkTimer = null;
  }

  private clearTimers(): void {
    this.clearSaveTimer();
    this.clearCheckTimer();
  }

  private set(next: DraftSnapshot): void {
    this.snap = next;
    this.listeners.forEach((l) => l());
  }
}
