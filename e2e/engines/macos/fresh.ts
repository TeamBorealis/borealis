import { execFile } from 'node:child_process';
import { readdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { promisify } from 'node:util';
import { ConfigurationError } from 'e2e/engine';

const exec = promisify(execFile);
const WATCHED = ['ui', 'src-tauri', 'crates'] as const;

/** Fails when the release bundle predates the newest commit that touched the app sources (e2e-only commits do not count) or the working tree has changes in the app's source dirs. */
export async function assertBundleFresh(appPath: string, e2eRoot: string): Promise<void> {
  const appRoot = dirname(e2eRoot) === e2eRoot ? e2eRoot : join(e2eRoot, '..');
  const git = (args: string[]) => exec('git', ['-C', appRoot, ...args]).then((r) => r.stdout.trim());
  const built = statSync(join(appPath, 'Contents/MacOS')).mtimeMs / 1000;
  const head = Number(await git(['log', '-1', '--format=%ct', '--', ...WATCHED]));
  const dirty = (await git(['status', '--porcelain', '-uall', '--', ...WATCHED])).split('\n').filter(Boolean).map((l) => l.slice(3));
  const newest = Math.max(0, ...dirty.map((f) => { try { return statSync(join(appRoot, f)).mtimeMs / 1000; } catch { return 0; } }));
  const stale = built < head ? `older than the last commit touching ui/, src-tauri/ or crates/ (${new Date(head * 1000).toISOString()})` : built < newest ? 'older than uncommitted changes in ui/, src-tauri/ or crates/' : '';
  if (!stale) return;
  throw new ConfigurationError('INVALID_CONFIG', `Borealis.app is stale: built ${new Date(built * 1000).toISOString()}, ${stale}. Rebuild: cd app && source ~/.cargo/env && npm run tauri -- build (docs/build.md)`);
}

/**
 * Removes device claims that earlier e2e runs left behind (the agent-device daemon outlives the run, so the claim owner
 * stays alive). The macOS GUI is single-user: any other borealis-e2e-* claim is stale by definition.
 */
export function clearStaleClaims(home: string, ownSession: string): number {
  const dir = join(home, '.agent-device/device-claims');
  let files: string[] = [];
  try { files = readdirSync(dir); } catch { return 0; }
  let cleared = 0;
  for (const f of files) {
    try {
      const claim = JSON.parse(readFileSync(join(dir, f), 'utf8')) as { session?: string };
      if (!claim.session?.startsWith('borealis-e2e-') || claim.session === ownSession) continue;
      rmSync(join(dir, f));
      cleared += 1;
    } catch { /* unreadable claim: leave it */ }
  }
  return cleared;
}
