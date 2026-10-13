import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';

const run = promisify(execFile);
const NATIVE_DIR = join(import.meta.dirname, 'native');
const CACHE_DIR = join(tmpdir(), 'borealis-e2e-native');

/** Compiles `native/<name>.swift` once per source revision and returns the binary path. */
async function binary(name: string): Promise<string> {
  const source = join(NATIVE_DIR, `${name}.swift`);
  const hash = createHash('sha256').update(readFileSync(source)).digest('hex').slice(0, 12);
  const out = join(CACHE_DIR, `${name}-${hash}`);
  if (existsSync(out)) return out;
  mkdirSync(CACHE_DIR, { recursive: true });
  await run('swiftc', ['-O', '-o', out, source], { timeout: 120_000 });
  return out;
}

export interface Permissions { readonly accessibility: boolean; readonly screenRecording: boolean }

/** Reads the grants without prompting. The caller decides what to do when one is missing. */
export async function probePermissions(): Promise<Permissions> {
  const { stdout } = await run(await binary('tcc-probe'), [], { timeout: 15_000 });
  return { accessibility: /accessibility=true/.test(stdout), screenRecording: /screenrecording=true/.test(stdout) };
}

export async function sendKeyCode(code: number, modifiers: readonly string[]): Promise<void> {
  await run(await binary('keysend'), [String(code), ...modifiers], { timeout: 10_000 });
}

/** A real left click at a screen point (count 2 = double click). */
export async function clickAt(x: number, y: number, count = 1): Promise<void> {
  await run(await binary('click'), [String(x), String(y), String(count)], { timeout: 10_000 });
}

/** The exact instructions for the user; nothing here changes TCC. */
export function permissionMessage(missing: Permissions): string {
  const lines = ['Borealis macOS e2e cannot drive the desktop yet: macOS privacy grants are missing.'];
  const host = process.env['TERM_PROGRAM'] ?? 'the app that launched this run (the terminal or IDE host)';
  if (!missing.accessibility) lines.push(`- Accessibility: System Settings > Privacy & Security > Accessibility > enable ${host} (the parent of node, which owns agent-device's macos helper).`);
  if (!missing.screenRecording) lines.push(`- Screen Recording: System Settings > Privacy & Security > Screen & System Audio Recording > enable ${host}.`);
  lines.push('Then quit and reopen that app (grants apply at launch) and rerun. If it still fails, also add ~/.agent-device/macos-helper/current/agent-device-macos-helper with the "+" button (Cmd+Shift+G to type the path).');
  return lines.join('\n');
}
