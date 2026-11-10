import type { Api, ApiError, HomebrewTrialChange, HomebrewTrialSetup, HomebrewTrialView } from "../../../api/contract";
import { errorMessage } from "../../../state/useAsync";

export type TrialApi = Pick<Api, "homebrewTrialStart" | "homebrewTrialUpdate" | "homebrewTrialEnd">;

export interface TrialState {
  view: HomebrewTrialView | null;
  /** A start or an update is in flight. */
  busy: boolean;
  error: string | null;
  /** A plain line for the flyout (the test character was made again after it expired). */
  notice: string | null;
  /** Goes up with every new view: the Build pickers reload their options on it (GitHub #7). */
  revision: number;
}

interface Waiter { resolve: (v: HomebrewTrialView) => void; reject: (e: unknown) => void }
interface Queued { change: HomebrewTrialChange; waiters: Waiter[] }

const EXPIRED_NOTICE = "The test character had been closed after a while without use, so it was made again.";

/** Level and setup changes can travel as one update; a choice (slot) always travels alone. */
const mergeable = (c: HomebrewTrialChange): boolean => c.slotId === undefined;

export function mergeChanges(a: HomebrewTrialChange, b: HomebrewTrialChange): HomebrewTrialChange {
  const setup = a.setup || b.setup ? { ...a.setup, ...b.setup } : undefined;
  const level = b.level ?? a.level;
  return { ...(level !== undefined ? { level } : {}), ...(setup ? { setup } : {}) };
}

const isNotFound = (e: unknown): boolean => (e as Partial<ApiError> | null)?.code === "not_found";

/**
 * One Try it session (spec 9): starts the backend's trial character, sends changes one at a time (level and setup changes
 * that wait are merged, so stepping the level quickly sends one update), and ends it on close. Answers that arrive after
 * the close are dropped and their trial ended. A trial the backend dropped (30 minutes idle) is started again with the
 * same setup and level, and the change is sent once more.
 */
export class TrialSession {
  private state: TrialState = { view: null, busy: false, error: null, notice: null, revision: 0 };
  private listeners = new Set<() => void>();
  private queue: Queued[] = [];
  private sending = false;
  private starting: Promise<void> | null = null;
  private closed = false;
  /** The setup as the user set it (sent again when the trial must be made again). */
  private setup: HomebrewTrialSetup = {};

  constructor(private api: TrialApi, private draftId: string) {}

  subscribe = (fn: () => void): (() => void) => {
    this.listeners.add(fn);
    return () => { this.listeners.delete(fn); };
  };

  getSnapshot = (): TrialState => this.state;

  private set(patch: Partial<TrialState>): void {
    this.state = { ...this.state, ...patch };
    this.listeners.forEach((fn) => fn());
  }

  private adopt(view: HomebrewTrialView): void {
    this.set({ view, error: null, revision: this.state.revision + 1 });
  }

  start(setup: HomebrewTrialSetup = {}): Promise<void> {
    this.setup = { ...setup };
    this.set({ busy: true });
    this.starting = this.api.homebrewTrialStart({ draftId: this.draftId, setup: this.setup })
      .then((view) => {
        if (this.closed) { void this.api.homebrewTrialEnd({ trialId: view.trialId }).catch(() => undefined); return; }
        this.adopt(view);
      })
      .catch((e: unknown) => { if (!this.closed) this.set({ error: errorMessage(e) }); })
      .finally(() => {
        this.starting = null;
        if (!this.closed) this.set({ busy: this.queue.length > 0 });
        void this.pump();
      });
    return this.starting;
  }

  /** Sends a change; resolves with the view that carries it, rejects with the backend's error. */
  change(change: HomebrewTrialChange): Promise<HomebrewTrialView> {
    if (this.closed) return Promise.reject(new Error("Try it is closed."));
    return new Promise((resolve, reject) => {
      const waiter = { resolve, reject };
      const last = this.queue[this.queue.length - 1];
      if (last && mergeable(last.change) && mergeable(change)) {
        this.queue[this.queue.length - 1] = { change: mergeChanges(last.change, change), waiters: [...last.waiters, waiter] };
      } else {
        this.queue.push({ change, waiters: [waiter] });
      }
      this.set({ busy: true });
      void this.pump();
    });
  }

  private async pump(): Promise<void> {
    if (this.sending || this.starting || this.closed) return;
    const item = this.queue.shift();
    if (!item) { this.set({ busy: false }); return; }
    this.sending = true;
    try {
      const view = await this.send(item.change);
      if (!this.closed) this.adopt(view);
      item.waiters.forEach((w) => w.resolve(view));
    } catch (e) {
      if (!this.closed) this.set({ error: errorMessage(e) });
      item.waiters.forEach((w) => w.reject(e));
    } finally {
      this.sending = false;
      void this.pump();
    }
  }

  private async send(change: HomebrewTrialChange): Promise<HomebrewTrialView> {
    const view = this.state.view;
    if (!view) throw new Error(this.state.error ?? "The test character could not be made.");
    this.setup = { ...this.setup, ...change.setup, ...(change.level !== undefined ? { level: change.level } : {}) };
    try {
      return await this.api.homebrewTrialUpdate({ trialId: view.trialId, change });
    } catch (e) {
      if (!isNotFound(e)) throw e;
      const level = this.setup.level ?? view.snapshot.level.total;
      const fresh = await this.api.homebrewTrialStart({ draftId: this.draftId, setup: { ...this.setup, level } });
      this.set({ notice: EXPIRED_NOTICE });
      return change.slotId ? this.api.homebrewTrialUpdate({ trialId: fresh.trialId, change }) : fresh;
    }
  }

  /** Ends the trial (spec 9: dropped on close); waiting changes are refused. */
  async end(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    const waiting = this.queue;
    this.queue = [];
    waiting.forEach((q) => q.waiters.forEach((w) => w.reject(new Error("Try it is closed."))));
    const id = this.state.view?.trialId;
    if (id) await this.api.homebrewTrialEnd({ trialId: id }).catch(() => undefined);
  }
}
