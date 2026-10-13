import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

/**
 * Per-user locations Borealis (bundle id app.borealis) persists to. These are the ONLY paths this
 * module ever deletes, and only after the originals were moved into the backup (see `resetData`).
 */
export const MANAGED_PATHS = [
  'Library/Application Support/app.borealis',
  'Library/WebKit/app.borealis',
  'Library/Caches/app.borealis',
  'Library/Saved Application State/app.borealis.savedState',
] as const;

interface Entry { readonly original: string; readonly saved: string }
interface Marker { readonly runId: string; readonly startedAt: string; readonly entries: readonly Entry[] }

const BACKUP_DIR = '.borealis-e2e-backup';

/** The app installs read-only trees (ai-profile/profile); rm cannot enter them until the owner can write. */
function forceRemove(path: string): void {
  if (!existsSync(path)) return;
  try { execFileSync('chmod', ['-R', 'u+w', path]); } catch { /* removal below reports the real problem */ }
  rmSync(path, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
}

function backupRoot(home: string): string { return join(home, BACKUP_DIR); }
function markerPath(home: string): string { return join(backupRoot(home), 'marker.json'); }

function readMarker(home: string): Marker | undefined {
  try { return JSON.parse(readFileSync(markerPath(home), 'utf8')) as Marker; } catch { return undefined; }
}

/** True while a backup of the user's real data is outstanding (a run in progress, or one that crashed). */
export function hasBackup(home: string = homedir()): boolean { return existsSync(markerPath(home)); }

/**
 * Moves the user's real app data aside (rename, same volume, atomic) and leaves the app with a clean
 * slate. The marker is written BEFORE anything moves, so a crash at any point is recoverable. A backup left
 * by a crashed earlier run is restored first, so a second run can never overwrite the user's data with
 * test data.
 */
export function backupAndClear(runId: string, home: string = homedir()): { readonly moved: number } {
  if (hasBackup(home)) restore(home);
  const root = backupRoot(home);
  mkdirSync(join(root, 'data'), { recursive: true });
  const entries = MANAGED_PATHS.map((rel, i): Entry => ({ original: join(home, rel), saved: join(root, 'data', String(i)) }));
  const marker: Marker = { runId, startedAt: new Date().toISOString(), entries };
  writeFileSync(markerPath(home), JSON.stringify(marker, null, 2));
  let moved = 0;
  for (const entry of entries) {
    if (!existsSync(entry.original)) continue;
    renameSync(entry.original, entry.saved);
    moved += 1;
  }
  return { moved };
}

/**
 * Deletes the app's data for a clean test. Refuses unless a backup marker exists: user data that was
 * not moved aside first is never deleted.
 */
export function resetData(home: string = homedir()): void {
  const marker = readMarker(home);
  if (marker === undefined) throw new Error('refusing to reset Borealis data: no backup marker, the real data would be lost');
  for (const entry of marker.entries) forceRemove(entry.original);
}

/** Puts the user's data back, replacing whatever the tests left, and removes the backup. Idempotent. */
export function restore(home: string = homedir()): { readonly restored: number } {
  const marker = readMarker(home);
  if (marker === undefined) return { restored: 0 };
  let restored = 0;
  for (const entry of marker.entries) {
    if (!existsSync(entry.saved)) {
      // Nothing was saved for this path: the user had none, so the test's own copy goes.
      forceRemove(entry.original);
      continue;
    }
    forceRemove(entry.original);
    mkdirSync(join(entry.original, '..'), { recursive: true });
    renameSync(entry.saved, entry.original);
    restored += 1;
  }
  rmSync(backupRoot(home), { recursive: true, force: true });
  return { restored };
}

/** Marks the first-run AI setup as done (the app only checks that the marker file exists), so suites start on Start. */
export function seedAiSetupDone(home: string = homedir()): void {
  const dir = join(home, 'Library/Application Support/app.borealis');
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'ai-setup.json'), JSON.stringify({ completed: true, skipped: true }));
}
