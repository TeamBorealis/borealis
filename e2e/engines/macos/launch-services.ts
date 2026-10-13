import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const exec = promisify(execFile);
const LSREGISTER = '/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister';

/**
 * Open-by-bundle-id (what agent-device does) starts ANY installed copy with that id, e.g. /Applications/Borealis.app,
 * not the build under test. Unregisters every other real .app copy for the run (files are never touched); `showCopies` puts
 * them back. Copies on mounted volumes and simulator builds are left alone.
 */
export async function hideOtherCopies(appPath: string, bundleId: string): Promise<readonly string[]> {
  const { stdout } = await exec('mdfind', [`kMDItemCFBundleIdentifier == '${bundleId}'`]);
  const others = stdout.split('\n').filter((p) => p.endsWith('.app') && p !== appPath && p.startsWith('/Applications/'));
  for (const p of others) await exec(LSREGISTER, ['-u', p]).catch(() => undefined);
  return others;
}

export async function showCopies(paths: readonly string[]): Promise<void> {
  for (const p of paths) await exec(LSREGISTER, ['-f', p]).catch(() => undefined);
}
