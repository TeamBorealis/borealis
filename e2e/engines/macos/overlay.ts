import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { responsivePids } from './native.ts';

const run = promisify(execFile);

/**
 * macOS's Automation Mode overlay: a full-screen window (level 1000) shown while XCTest drives the Mac. Before every
 * typed key XCTest checks the windows above the target for interruptions, so it asks this process over accessibility.
 * On macOS 27.0.1 the overlay can deadlock in its event tap (SkyLight `CGSEventSourceForID` -> `CGSEventSourceShutdown`
 * waits on a mutex it already holds); it keeps its window up and never answers again, and every XCTest `typeText`
 * then hangs until agent-device's runner gives up with "main thread execution timed out".
 */
export const OVERLAY_PROCESS = 'AutomationModeUI';

export interface OverlayDeps {
  /** The overlay's running pids. */
  readonly findPids: () => Promise<readonly number[]>;
  /** The subset of `pids` that answered an accessibility request in time. */
  readonly responsive: (pids: readonly number[]) => Promise<ReadonlySet<number>>;
  readonly kill: (pid: number) => Promise<void>;
}

const defaultDeps: OverlayDeps = {
  async findPids() {
    try {
      const { stdout } = await run('pgrep', ['-x', OVERLAY_PROCESS]);
      return stdout.split('\n').map(Number).filter((n) => Number.isInteger(n) && n > 0);
    } catch {
      return []; // pgrep exits 1 when nothing matches
    }
  },
  responsive: responsivePids,
  async kill(pid) {
    try { process.kill(pid, 'SIGKILL'); } catch { /* already gone */ }
  },
};

/**
 * Kills every overlay process that no longer answers accessibility (a deadlocked one ignores SIGTERM) and returns
 * their pids. launchd starts a fresh overlay with the next XCTest session. A responsive overlay is left alone.
 */
export async function recoverWedgedOverlay(deps: OverlayDeps = defaultDeps): Promise<readonly number[]> {
  const pids = await deps.findPids();
  if (pids.length === 0) return [];
  const alive = await deps.responsive(pids);
  const wedged = pids.filter((pid) => !alive.has(pid));
  for (const pid of wedged) await deps.kill(pid);
  return wedged;
}

/** Whether an agent-device error is its runner giving up on XCTest work (the symptom of a wedged overlay). */
export function isMainThreadTimeout(err: unknown): boolean {
  const message = err instanceof Error ? err.message : String(err);
  return message.toLowerCase().includes('main thread execution timed out');
}

/** The notice for a run log when the engine killed a wedged overlay. */
export function wedgedOverlayNotice(pids: readonly number[]): string {
  return `macOS ${OVERLAY_PROCESS} (pid ${pids.join(', ')}) stopped answering accessibility, which hangs XCTest typing; ` +
    'the engine killed it (launchd starts a fresh one)';
}
