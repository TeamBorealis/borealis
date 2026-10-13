import { describe, test } from '@e2e-dev/mobile';
import { expect } from 'e2e';
import type { Device } from '@e2e-dev/mobile';

/**
 * Clean slate for a real-backend test: remove the app (and its whole data dir) and install the
 * build again. `app.clearState()` is avoided because agent-device cannot pick the simulator for
 * its reset when a second simulator is booted ("2 devices match this request equally").
 */
export async function freshInstall(device: Device): Promise<void> {
  await device.installApp(undefined, { reinstall: true });
}

/** Assertion budget for screens that follow a launch or a Rust-backed navigation (slow AX tree). */
export const SLOW = { timeout: 60_000 } as const;

export const CHARACTER_NAME = 'Mira Test';

type Screen = Parameters<Parameters<typeof import('@e2e-dev/mobile').test>[1]>[0]['screen'];

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

/** Build page after Create: the status bar prompts for the pending species ('race') choice. */
export async function expectBuildPage(screen: Screen): Promise<void> {
  await expect(screen.getByText(/CHOOSE ONE FROM THE 'RACE' SELECTION/)).toBeVisible(SLOW);
}

export type MobilePlatform = 'ios' | 'android';

/**
 * A bottom-bar tab (a `role="link"` with an exact accessible name). iOS reports the link role. On Android
 * the engine maps Chromium's `android.view.View` link to `group` and reads no "link" role description, so
 * the same node is matched by its exact, case-sensitive accessible name ("PLAY" text nodes do not match).
 */
export function navLink(screen: Screen, platform: MobilePlatform, name: RegExp) {
  return platform === 'ios' ? screen.getByRole('link', name) : screen.getByLabel(name);
}

type Context = Parameters<Parameters<typeof test>[1]>[0];
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
