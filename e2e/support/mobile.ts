import { execFile } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { describe, test } from '@e2e-dev/mobile';
import { expect } from 'e2e';
import type { TestFixtures } from 'e2e';
import type { Device } from '@e2e-dev/mobile';
import { ANDROID_BUNDLE_ID, findAndroidApk } from './mobile-env.js';

const ADB = join(process.env.ANDROID_HOME ?? join(homedir(), 'Library/Android/sdk'), 'platform-tools/adb');
/** A 1.6 GB debug APK takes minutes to stream onto the emulator. */
const ADB_INSTALL_TIMEOUT_MS = 10 * 60_000;

/**
 * Clean slate for a real-backend test: remove the app (and its whole data dir) and install the
 * build again. `app.clearState()` is avoided because agent-device cannot pick the simulator for
 * its reset when a second simulator is booted ("2 devices match this request equally").
 *
 * On Android the engine's reinstall fails when the app is missing ("adb uninstall failed",
 * DELETE_FAILED_INTERNAL_ERROR), and a debug APK of over 1.5 GB does not fit the AVD's /data when it
 * is copied to the device before installing (INSTALL_FAILED_INSUFFICIENT_STORAGE). Then adb removes
 * the app itself and streams the same APK (`adb install`), which needs no second copy.
 */
export async function freshInstall(device: Device): Promise<void> {
  try {
    await device.installApp(undefined, { reinstall: true });
  } catch (e) {
    const apk = findAndroidApk();
    if (!apk || !/adb|INSTALL_FAILED_INSUFFICIENT_STORAGE/.test(String(e))) throw e;
    await execFileAsync(ADB, ['uninstall', ANDROID_BUNDLE_ID]).catch(() => undefined);
    await execFileAsync(ADB, ['install', '-r', '-t', apk], { timeout: ADB_INSTALL_TIMEOUT_MS });
  }
}

/** Assertion budget for screens that follow a launch or a Rust-backed navigation (slow AX tree). */
export const SLOW = { timeout: 60_000 } as const;

export const CHARACTER_NAME = 'Mira Test';

const execFileAsync = promisify(execFile);

type Screen = TestFixtures['screen'];

/** Start page -> New character dialog -> name -> Create. Leaves the app on the Build page. */
export async function createCharacter(screen: Screen, device: Device, name: string): Promise<void> {
  const field = screen.getByRole('textbox');
  // A tap right after launch can land before the page is interactive (seen on Android): retry once.
  for (let attempt = 0; ; attempt += 1) {
    await screen.getByRole('button', /create a new character/i).tap();
    try {
      await expect(field).toBeVisible({ timeout: 10_000 });
      break;
    } catch (e) {
      if (attempt >= 2) throw e;
    }
  }
  await field.fill(name);
  await device.dismissKeyboard();
  await screen.getByRole('button', /^create character$/i).tap();
}

/** The saved character's tile in the Start list: a button named "<Name>, Level <n> ..." (one node; the group and text only repeat the name). */
export function characterEntry(screen: Screen, name: string) {
  return screen.getByRole('button', new RegExp(`^${name}, Level \\d+`));
}

/**
 * Build page after Create: the status bar prompts for the pending species ('race') choice, in Aurora's words on the wide
 * shell and as "Next: choose Species (Build tab, Species)" on the phone shell (tester B7).
 */
export async function expectBuildPage(screen: Screen): Promise<void> {
  await expect(screen.getByText(/choose one from the 'race' selection|^next: choose (race|species)\b/i).first()).toBeVisible(SLOW);
}

export type MobilePlatform = 'ios' | 'android';

/**
 * Which shell the app draws (ui/src/components/shell, U9b): `phone` is the narrow shell (hamburger title
 * bar, bottom tab bar), always on a phone; `wide` is the desktop-like shell (File/View/Help menu bar, upper-case
 * main tab strip) a tablet such as the iPad shows at full width. Tests branch on this, never on device names.
 */
export type Shell = 'phone' | 'wide';

const SHELL_POLL_MS = 1_000;

/**
 * Waits for the first screen and reports its shell. The phone shell's bottom bar has a "Start" tab (the text the
 * phone tests already wait on); the wide shell has no bottom bar and its title bar has a "Project website" button
 * the phone title bar lacks (MobileChrome.tsx vs Chrome.tsx). Its main tab reads "START", which the case-sensitive
 * phone marker does not match, so the first one seen decides.
 */
export async function waitForShell(screen: Screen): Promise<Shell> {
  const wide = screen.getByRole('button', /^Project website$/);
  const phone = screen.getByText(/^Start$/).last();
  const end = Date.now() + SLOW.timeout;
  for (;;) {
    if (await phone.isVisible().catch(() => false)) return 'phone';
    if (await wide.isVisible().catch(() => false)) return 'wide';
    if (Date.now() > end) throw new Error('neither the phone bottom bar (Start) nor the wide title bar (Project website) showed');
    await new Promise<void>((r) => setTimeout(r, SHELL_POLL_MS));
  }
}

/**
 * A main tab. Phone shell: a bottom-bar tab (a `role="link"` with an exact accessible name). iOS reports the link
 * role. On Android the engine maps Chromium's `android.view.View` link to `group` and reads no "link" role
 * description, so the same node is matched by its exact, case-sensitive accessible name ("PLAY" text nodes do not
 * match). Wide shell: the main tab strip, whose links iOS names by their upper-cased text ("PLAY"), so the name
 * is matched without case.
 */
export function navLink(screen: Screen, platform: MobilePlatform, name: RegExp, shell: Shell = 'phone') {
  if (shell === 'wide') return screen.getByRole('link', new RegExp(name.source, `${name.flags.replace('i', '')}i`));
  return platform === 'ios' ? screen.getByRole('link', name) : screen.getByLabel(name);
}

/** The agent-device CLI next to the e2e package, and the session the mobile engine drives the device under. */
const AGENT_DEVICE = join(import.meta.dirname, '../node_modules/.bin/agent-device');
/** `e2e-<target>-<slot>` (@e2e-dev/mobile pool.js); the iOS target runs one device, slot 0. */
const IOS_ENGINE_SESSION = 'e2e-ios-0';
const NATIVE_GESTURE_TIMEOUT_MS = 60_000;

/**
 * A real iOS double-tap gesture on `target`, through the engine's own agent-device session. The engine's
 * `doubleTap()` is two separate presses (@e2e-dev/mobile actions.js); WebKit on an iPhone turns those into a
 * `dblclick`, but in the iPad's wide shell it does not (two presses only select the tile), while the system
 * double-tap gesture opens the character as a finger does (checked by hand on the iPad simulator).
 */
export async function nativeDoubleTap(target: ReturnType<Screen['getByRole']>): Promise<void> {
  const box = await target.boundingBox();
  if (!box) throw new Error('double tap target has no bounds');
  const x = Math.round(box.x + box.width / 2);
  const y = Math.round(box.y + box.height / 2);
  await execFileAsync(AGENT_DEVICE, ['press', String(x), String(y), '--double-tap', '--session', IOS_ENGINE_SESSION], {
    env: { ...process.env, DEVELOPER_DIR: process.env.DEVELOPER_DIR ?? '/Applications/Xcode.app/Contents/Developer' },
    timeout: NATIVE_GESTURE_TIMEOUT_MS,
  });
}

type Context = TestFixtures & { device: Device };
type Body = (context: Context, platform: MobilePlatform) => Promise<void>;

/** Registers one suite on iOS (runs) and on Android (runs); same body, strict assertions. */
export function mobileSuite(id: string, title: string, body: Body): void {
  describe(id, { tags: ['ios', id], platforms: ['ios'], requires: ['native-app'] }, () => {
    test(title, (context) => body(context, 'ios'));
  });
  describe(`${id}-android`, { tags: ['android', id], platforms: ['android'], requires: ['native-app'] }, () => {
    test(title, (context) => body(context, 'android'));
  });
}
