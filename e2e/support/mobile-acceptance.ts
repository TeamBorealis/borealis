import { execFileSync, spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';
import { m4MetricLines } from './mobile-metrics.js';
import { ANDROID_BUNDLE_ID, IOS_BUNDLE_ID, IOS_SIMULATOR } from './mobile-env.js';
import type { MobilePlatform } from './mobile.js';

/**
 * Device-side plumbing for m4 (S5 mobile acceptance, specs/acceptance-imports-1.md): putting the input
 * zips where the native file chooser lists them, the debug-only `acceptance_digest` request/answer files
 * in the app's data folder, and peak memory. Android goes through adb (`run-as` works on the debug APK),
 * iOS through simctl and the host file system (the simulator's files are host files).
 */

/** Same names as `acceptance_digest::REQUEST_FILE` / `RESULT_FILE` (src-tauri/src/acceptance_digest.rs). */
const REQUEST_FILE = 'acceptance-digest.request';
const RESULT_FILE = 'acceptance-digest.json';

const ADB = join(process.env.ANDROID_HOME ?? join(homedir(), 'Library/Android/sdk'), 'platform-tools/adb');
const DEVELOPER_DIR = process.env.DEVELOPER_DIR ?? '/Applications/Xcode.app/Contents/Developer';

function run(cmd: string, args: string[]): string {
  return execFileSync(cmd, args, { encoding: 'utf8', env: { ...process.env, DEVELOPER_DIR }, maxBuffer: 64 << 20, stdio: ['ignore', 'pipe', 'pipe'] });
}

const adb = (...args: string[]) => run(ADB, args);
const simctl = (...args: string[]) => run('xcrun', ['simctl', ...args]);
const runAs = (script: string) => adb('shell', 'run-as', ANDROID_BUNDLE_ID, 'sh', '-c', `'${script}'`);

/** The app's data folder (the one holding `library/`), relative to the run-as home on Android. */
function androidDataDir(): string {
  for (const dir of ['.', 'files', 'app_data']) {
    const out = runAs(`[ -d ${dir}/library ] && echo yes || true`);
    if (out.includes('yes')) return dir;
  }
  throw new Error('Android data folder with library/ not found');
}

function iosDataDir(): string {
  const container = simctl('get_app_container', IOS_SIMULATOR, IOS_BUNDLE_ID, 'data').trim();
  return join(container, 'Library/Application Support', IOS_BUNDLE_ID);
}

/** The simulator's "On My iPhone" folder (Files app local storage), created when Files first ran. */
function iosOnMyIphone(): string {
  const groups = join(homedir(), 'Library/Developer/CoreSimulator/Devices', IOS_SIMULATOR, 'data/Containers/Shared/AppGroup');
  const stores = readdirSync(groups)
    .map((g) => join(groups, g, 'File Provider Storage'))
    .filter((p) => existsSync(p));
  // several app groups carry the folder; the Files app's own one already holds files once used
  const used = stores.find((p) => readdirSync(p).some((f) => !f.startsWith('.')));
  if (used ?? stores[0]) return (used ?? stores[0]) as string;
  throw new Error('Files app storage not found: open the Files app once on this simulator');
}

/** Copy the zips where the platform's document picker shows them (Android Downloads, iOS On My iPhone). */
export function deliverInputs(platform: MobilePlatform, files: string[]): void {
  if (platform === 'android') {
    for (const f of files) {
      adb('push', f, `/sdcard/Download/${basename(f)}`);
      adb('shell', 'am', 'broadcast', '-a', 'android.intent.action.MEDIA_SCANNER_SCAN_FILE', '-d', `file:///sdcard/Download/${basename(f)}`);
    }
    return;
  }
  const dir = iosOnMyIphone();
  for (const f of files) copyFileSync(f, join(dir, basename(f)));
}

/** Ask the next launch for the digest (and drop an old answer). */
export function requestDigest(platform: MobilePlatform): void {
  if (platform === 'android') {
    const dir = androidDataDir();
    runAs(`rm -f ${dir}/${RESULT_FILE} && touch ${dir}/${REQUEST_FILE}`);
    return;
  }
  const dir = iosDataDir();
  rmSync(join(dir, RESULT_FILE), { force: true });
  writeFileSync(join(dir, REQUEST_FILE), '');
}

/** The digest the app wrote, or undefined while it is still computing. */
export function readDigest(platform: MobilePlatform): Record<string, unknown> | undefined {
  try {
    const text = platform === 'android'
      ? runAs(`cat ${androidDataDir()}/${RESULT_FILE}`)
      : readFileSync(join(iosDataDir(), RESULT_FILE), 'utf8');
    return text.trim() ? JSON.parse(text) : undefined;
  } catch {
    return undefined;
  }
}

function androidPid(): string | undefined {
  const pid = adb('shell', 'pidof', ANDROID_BUNDLE_ID).trim();
  return pid || undefined;
}

function iosPid(): string | undefined {
  const lines = run('ps', ['-axo', 'pid=,command=']).split('\n');
  const hit = lines.find((l) => l.includes(IOS_SIMULATOR) && l.includes('/Borealis.app/Borealis'));
  return hit?.trim().split(/\s+/)[0];
}

/**
 * Peak resident memory of the running app process in KiB, since it started: Android `VmHWM` from
 * /proc/<pid>/status; iOS the simulator process's `phys_footprint_peak` (macOS `footprint`).
 */
export function peakMemoryKb(platform: MobilePlatform): number | undefined {
  if (platform === 'android') {
    const pid = androidPid();
    if (!pid) return undefined;
    const status = (() => {
      try { return adb('shell', 'cat', `/proc/${pid}/status`); } catch { return runAs(`cat /proc/${pid}/status`); }
    })();
    const m = /VmHWM:\s+(\d+)\s+kB/.exec(status);
    return m ? Number(m[1]) : undefined;
  }
  const pid = iosPid();
  if (!pid) return undefined;
  const out = run('footprint', ['-p', pid, '--noCategories', '-f', 'bytes']);
  const m = /phys_footprint_peak:\s+(\d+)/.exec(out);
  return m ? Math.round(Number(m[1]) / 1024) : undefined;
}

/** Write one platform's m4 result next to the other outputs. */
export function writeResult(outDir: string, platform: MobilePlatform, result: unknown): string {
  mkdirSync(outDir, { recursive: true });
  const file = join(outDir, `m4-${platform}.json`);
  writeFileSync(file, JSON.stringify(result, null, 2));
  return file;
}

/**
 * The document picker's grid cell of `name` in "On My iPhone" (iOS draws the picker out of process, so its
 * items are not in the app's accessibility tree and are tapped by position). Files sorts by name; the
 * grid has 3 columns. Geometry of the iPhone 18 Pro simulator (402x874 points).
 */
const IOS_GRID = { columns: [70, 201, 329], firstRowY: 240, rowHeight: 198 } as const;

export function iosPickerCell(name: string): { x: number; y: number } {
  const items = readdirSync(iosOnMyIphone())
    .filter((f) => !f.startsWith('.'))
    .map((f) => f.replace(/\.(xml|index)$/i, ''))
    .sort((a, b) => a.localeCompare(b, 'en', { sensitivity: 'base', numeric: true }));
  const at = items.indexOf(name.replace(/\.(xml|index)$/i, ''));
  if (at < 0) throw new Error(`${name} is not in On My iPhone`);
  const cols = IOS_GRID.columns.length;
  return { x: IOS_GRID.columns[at % cols] as number, y: IOS_GRID.firstRowY + IOS_GRID.rowHeight * Math.floor(at / cols) };
}

/** The picker's Browse tab, its "On My iPhone" location row and its Open button (multi-select pickers only). */
export const IOS_PICKER_BROWSE = { x: 288, y: 815 } as const;
export const IOS_PICKER_ON_MY_IPHONE = { x: 125, y: 308 } as const;
export const IOS_PICKER_OPEN = { x: 348, y: 100 } as const;

/** Width of the simulator screen in points (the tap coordinates above). */
const IOS_POINTS_WIDTH = 402;
/** The Open button's box in points, and how many of its pixels must be the system blue. */
const IOS_OPEN_BOX = { x0: 318, x1: 384, y0: 86, y1: 114 } as const;
const MIN_BLUE_PIXELS = 50;

/**
 * Whether the picker shows Open (a file is selected) rather than its close button: the picker is not in
 * the accessibility tree, so this reads the simulator's screen (`simctl io screenshot`, 32-bit BMP).
 */
export function iosOpenButtonShown(): boolean {
  const file = join(tmpdir(), `borealis-m4-${process.pid}.bmp`);
  simctl('io', IOS_SIMULATOR, 'screenshot', '--type=bmp', file);
  const bmp = readFileSync(file);
  rmSync(file, { force: true });
  const offset = bmp.readUInt32LE(10);
  const width = bmp.readInt32LE(18);
  const height = bmp.readInt32LE(22);
  const scale = width / IOS_POINTS_WIDTH;
  let blue = 0;
  for (let y = Math.round(IOS_OPEN_BOX.y0 * scale); y < IOS_OPEN_BOX.y1 * scale; y += 1) {
    const row = height < 0 ? y : height - 1 - y;
    for (let x = Math.round(IOS_OPEN_BOX.x0 * scale); x < IOS_OPEN_BOX.x1 * scale; x += 1) {
      const v = bmp.readUInt32LE(offset + (row * width + x) * 4);
      const [r, b] = [(v >> 16) & 0xff, v & 0xff];
      if (b > 200 && r < 160) blue += 1;
    }
  }
  return blue >= MIN_BLUE_PIXELS;
}

/** tools/ci/check_thresholds.py, the same checker CI uses for desktop. */
const CHECK_THRESHOLDS = resolve(import.meta.dirname, '../../tools/ci/check_thresholds.py');

/**
 * Writes the m4 METRIC lines to `<out>/m4-<platform>.metrics.log` and runs check_thresholds.py on them
 * (the checker CI runs on the desktop lines; limits: tools/acceptance/thresholds.json, numbers only).
 * Returns the checker's exit code and output; the numbers stay in the log, not in any report.
 */
export function checkThresholds(
  platform: MobilePlatform,
  times: Record<string, number>,
  peakKb: Record<string, number | undefined>,
  outDir: string,
): { code: number; output: string } {
  mkdirSync(outDir, { recursive: true });
  const log = join(outDir, `m4-${platform}.metrics.log`);
  writeFileSync(log, `${m4MetricLines(times, peakKb).join('\n')}\n`);
  const r = spawnSync('python3', ['-I', CHECK_THRESHOLDS, '--platform', platform, log], { encoding: 'utf8' });
  const output = `${r.stdout ?? ''}${r.stderr ?? ''}${r.error ? String(r.error) : ''}`.trim();
  return { code: r.status ?? 2, output };
}

/** Same name as `app_update_debug::FEED_FILE` (src-tauri/src/app_update_debug.rs). */
const UPDATE_FEED_FILE = 'debug-update-feed.json';

/**
 * B-41: put a release-feed fixture (GitHub latest-release JSON) in the app's data folder. A debug
 * build answers the update check from it on its next launch (release builds ignore the file).
 * The data folder exists once the app has launched after the install.
 */
export function writeUpdateFeed(platform: MobilePlatform, json: string): void {
  if (platform === 'android') {
    const local = join(tmpdir(), UPDATE_FEED_FILE);
    const remote = `/data/local/tmp/${UPDATE_FEED_FILE}`;
    writeFileSync(local, json);
    adb('push', local, remote);
    adb('shell', 'chmod', '644', remote);
    runAs(`cp ${remote} ${androidDataDir()}/${UPDATE_FEED_FILE}`);
    return;
  }
  writeFileSync(join(iosDataDir(), UPDATE_FEED_FILE), json);
}
