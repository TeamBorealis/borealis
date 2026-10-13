import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

export const IOS_BUNDLE_ID = 'app.borealis';
/** Use a dedicated simulator (override with `E2E_IOS_DEVICE`). */
const IOS_SIMULATOR_NAME = process.env.E2E_IOS_DEVICE ?? 'iPhone 18 Pro';

/**
 * agent-device refuses an ambiguous name ("iPhone 18 Pro" also matches "Pro Max"), so resolve the
 * exact-name simulator to its UDID. Falls back to the name when simctl is unavailable (web runs).
 */
function resolveSimulator(name: string): string {
  if (/^[0-9A-F-]{36}$/i.test(name)) return name;
  try {
    const out = execFileSync('xcrun', ['simctl', 'list', 'devices', 'available', '-j'], {
      encoding: 'utf8',
      env: { ...process.env, DEVELOPER_DIR: process.env.DEVELOPER_DIR ?? '/Applications/Xcode.app/Contents/Developer' },
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    const groups = JSON.parse(out).devices as Record<string, { name: string; udid: string }[]>;
    for (const list of Object.values(groups)) {
      const hit = list.find((d) => d.name === name);
      if (hit) return hit.udid;
    }
  } catch {
    // no Xcode: keep the name
  }
  return name;
}

export const IOS_SIMULATOR = resolveSimulator(IOS_SIMULATOR_NAME);

/**
 * Path of the simulator debug build from `npm run tauri -- ios build --debug --target aarch64-sim`.
 * E2E_IOS_APP overrides; otherwise the newest DerivedData/borealis-* product. Undefined when no
 * build exists, so loading the config (web runs) never needs one.
 */
export function findIosApp(): string | undefined {
  if (process.env.E2E_IOS_APP) return process.env.E2E_IOS_APP;
  const derived = join(homedir(), 'Library/Developer/Xcode/DerivedData');
  if (!existsSync(derived)) return undefined;
  const apps = readdirSync(derived)
    .filter((d) => d.startsWith('borealis-'))
    .map((d) => join(derived, d, 'Build/Products/debug-iphonesimulator/Borealis.app'))
    .filter((p) => existsSync(p))
    .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  return apps[0];
}

export const ANDROID_BUNDLE_ID = 'app.borealis';
/** AVD name the engine boots (see docs/build.md Android). */
export const ANDROID_AVD = process.env.E2E_ANDROID_DEVICE ?? 'borealis';

/**
 * Path of the debug APK from `npm run tauri -- android build --debug --target aarch64 --apk`.
 * E2E_ANDROID_APK overrides. Undefined when no build exists, so loading the config never needs one.
 */
export function findAndroidApk(): string | undefined {
  if (process.env.E2E_ANDROID_APK) return process.env.E2E_ANDROID_APK;
  const apk = join(
    import.meta.dirname,
    '../../src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk',
  );
  return existsSync(apk) ? apk : undefined;
}
