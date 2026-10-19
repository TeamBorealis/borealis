import { expect } from 'e2e';
import type { Device } from '@e2e-dev/mobile';
import type { MobilePlatform } from './mobile.js';
import { IOS_PICKER_BROWSE, IOS_PICKER_ON_MY_IPHONE, IOS_PICKER_OPEN, iosOpenButtonShown, iosPickerCell } from './mobile-acceptance.js';

type Screen = Parameters<Parameters<typeof import('@e2e-dev/mobile').test>[1]>[0]['screen'];
type Locator = ReturnType<Screen['getByText']>;

/** Swipes of the content page tried before a node counts as missing (the 5etools section is far down). */
const MAX_SWIPES = 30;
/** Swipes that bring the page back to its top first. */
const TOP_SWIPES = 6;
const SHORT = { timeout: 4_000 } as const;
/** The iOS picker animates in and out; its frames are not observable, so each step waits this long. */
const PICKER_SETTLE_MS = 3_000;
const PICK_SETTLE_MS = 1_500;
const PICK_ATTEMPTS = 4;
const pause = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** START > ADDITIONAL CONTENT (one stacked page: Custom content, Index files, 5etools import). */
export async function openAdditionalContentMobile(screen: Screen): Promise<void> {
  const tab = screen.getByText(/^additional content$/i).first();
  await expect(tab).toBeVisible({ timeout: 60_000 });
  for (let attempt = 0; attempt < 4; attempt += 1) {
    await tab.tap();
    try {
      await expect(screen.getByText(/^custom content$/i).first()).toBeVisible(SHORT);
      return;
    } catch { /* not switched yet */ }
  }
  await expect(screen.getByText(/^custom content$/i).first()).toBeVisible({ timeout: 60_000 });
}

/**
 * Scroll the page until `target` is on screen: Android's WebView only exposes on-screen nodes. Goes to
 * the top, then scrolls down (`direction: 'down'` moves the page down on a device) one step at a time.
 */
export async function scrollTo(screen: Screen, target: Locator): Promise<void> {
  if (await onScreen(screen, target)) return;
  for (let i = 0; i < TOP_SWIPES; i += 1) await screen.swipe({ direction: 'up' });
  for (let i = 0; i < MAX_SWIPES; i += 1) {
    if (await onScreen(screen, target)) return;
    await screen.swipe({ direction: 'down' });
  }
  await expect(target).toBeVisible(SHORT);
}

/**
 * Visible and between the section tab strip and the bottom tab bar: Android's tree also lists nodes
 * below the fold (under the bottom bar), and a tap on those is lost.
 */
async function onScreen(screen: Screen, target: Locator): Promise<boolean> {
  if (!(await target.isVisible())) return false;
  const boxOf = (l: Locator) => l.boundingBox().catch(() => null);
  const [box, top, bottom] = await Promise.all([
    boxOf(target),
    boxOf(screen.getByText(/^additional content$/i).first()),
    boxOf(screen.getByText(/^start$/i).last()),
  ]);
  if (!box || !top || !bottom) return true;
  return box.y >= top.y + top.height * 2 && box.y + box.height <= bottom.y - bottom.height * 2;
}

/**
 * Pick `name` in the native document picker the app just opened. Android DocumentsUI is in the tree: the
 * file when it is listed, else roots drawer > Downloads (where `deliverInputs` pushed it). The iOS picker
 * is not (out of process): Browse > On My iPhone, the file's grid cell, and Open when the picker allows
 * several files.
 */
export async function chooseFile(screen: Screen, device: Device, platform: MobilePlatform, name: string): Promise<void> {
  if (platform === 'ios') {
    // Browse twice: the first selects the tab (at its last folder), the second pops to its root list
    for (const point of [IOS_PICKER_BROWSE, IOS_PICKER_BROWSE, IOS_PICKER_ON_MY_IPHONE]) {
      await pause(PICKER_SETTLE_MS);
      await screen.tapAt(point);
    }
    await pause(PICKER_SETTLE_MS);
    // The first tap in a freshly opened folder is sometimes dropped, so tap until the pick shows: a
    // single-file picker closes (the app's tree is back), a multi-file one selects and offers Open.
    for (let attempt = 0; attempt < PICK_ATTEMPTS; attempt += 1) {
      await screen.tapAt(iosPickerCell(name));
      await pause(PICK_SETTLE_MS);
      if (await screen.getByText(/^additional content$/i).first().isVisible()) return;
      if (iosOpenButtonShown()) {
        await screen.tapAt(IOS_PICKER_OPEN);
        return;
      }
    }
    throw new Error(`the document picker did not take ${name}`);
  }
  const file = screen.getByText(name, { exact: true }).first();
  try {
    await expect(file).toBeVisible({ timeout: 8_000 });
  } catch {
    await device.locator('label="Show roots"').tap();
    await screen.getByText('Downloads', { exact: true }).first().tap();
    await expect(file).toBeVisible({ timeout: 30_000 });
  }
  await file.tap();
}

/**
 * A status line or report summary by its text: iOS exposes the text, Android's WebView the `role=status`
 * / summary group whose accessible name is that text.
 */
export function statusText(screen: Screen, platform: MobilePlatform, text: RegExp): Locator {
  return platform === 'ios' ? screen.getByText(text).first() : screen.getByLabel(text).first();
}
