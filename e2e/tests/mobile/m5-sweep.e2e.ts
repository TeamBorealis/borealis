import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { expect } from 'e2e';
import { describe, test } from '@e2e-dev/mobile';
import type { Device } from '@e2e-dev/mobile';
import type { TestFixtures } from 'e2e';
import { freshInstall, SLOW } from '../../support/mobile.js';
import type { MobilePlatform } from '../../support/mobile.js';
import { openAdditionalContentMobile, statusText } from '../../support/mobile-content.js';

// m5 (B-08 mobile sweep): the phone update notice (UP1/UP1a), Share logs (UP3) and an index download
// from a host with a bad TLS certificate (IM2). Opt-in (live network): E2E_B08=1 npm run e2e:ios|e2e:android -- --tag m5
//
// Update notice test hook: phones ask GitHub's latest-release API from the web view (`checkWithFetch`,
// CSP connect-src api.github.com only); there is no endpoint override. The test needs a build whose
// version (src-tauri/tauri.conf.json) is older than the latest published release, and checks that first.

const RELEASES_API = 'https://api.github.com/repos/alon-mini/borealis/releases/latest';
const UPDATE = 'Update Available';
/** Phones ask 12 s after launch; the answer then needs one round trip. */
const NOTICE_TIMEOUT = { timeout: 60_000 } as const;
/** Long enough for the delayed phone check to have run and shown the button if it were going to. */
const AFTER_CHECK_MS = 25_000;
const FOREGROUND_POLL_MS = 1_000;
const FOREGROUND_TIMEOUT_MS = 20_000;
/** A host whose certificate is signed by nobody (badssl.com test hosts). */
const BAD_CERT_INDEX = 'https://self-signed.badssl.com/core.index';
const FAIL_TIMEOUT = { timeout: 120_000 } as const;
const BUNDLE_ID = 'app.borealis';

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
type Context = TestFixtures & { device: Device };

function isNewer(latest: string, current: string): boolean {
  const [l, c] = [latest, current].map((v) => v.replace(/^v/, '').split('.').map(Number)) as [number[], number[]];
  const at = l.findIndex((n, i) => n !== (c[i] ?? 0));
  return at >= 0 && l[at]! > (c[at] ?? 0);
}

/** The latest published release is newer than this build; otherwise the notice cannot appear. */
async function requireNewerRelease(): Promise<string> {
  const conf = JSON.parse(readFileSync(resolve(import.meta.dirname, '../../../src-tauri/tauri.conf.json'), 'utf8')) as { version: string };
  const res = await fetch(RELEASES_API);
  if (!res.ok) throw new Error(`GitHub latest release: HTTP ${res.status}`);
  const tag = ((await res.json()) as { tag_name: string }).tag_name;
  if (!isNewer(tag, conf.version)) throw new Error(`precondition: build ${conf.version} is not older than the latest release ${tag}`);
  return tag.replace(/^v/, '');
}

async function waitForeignForeground(device: Device): Promise<string> {
  const end = Date.now() + FOREGROUND_TIMEOUT_MS;
  for (;;) {
    const id = (await device.foregroundApp().catch(() => undefined))?.bundleId;
    if (id && id !== BUNDLE_ID) return id;
    if (Date.now() > end) throw new Error(`Download left the app in the foreground (${id ?? 'unknown'})`);
    await sleep(FOREGROUND_POLL_MS);
  }
}

async function updateNotice({ app, device, screen }: Context, platform: MobilePlatform): Promise<void> {
  const latest = await requireNewerRelease();
  await freshInstall(device);
  await app.open();
  const button = screen.getByRole('button', UPDATE);
  await expect(button).toBeVisible(NOTICE_TIMEOUT);
  await button.tap();
  await expect(screen.getByText(new RegExp(`^Version ${latest.replace(/\./g, '\\.')} of Borealis is available\\.`))).toBeVisible(SLOW);
  // Download hands the website to the system browser. iOS: agent-device reports the session app, not
  // Safari, so the browser hand-off is only checked on Android (B-08 checked iOS on screen).
  await screen.getByRole('button', /^Download$/).tap();
  if (platform === 'android') console.info(`[m5] Download opened ${await waitForeignForeground(device)}`);
  await app.open();
  await expect(button).toBeVisible(NOTICE_TIMEOUT);
  await button.tap();
  // Close dismisses this version; the button is gone and stays gone after a restart.
  await screen.getByRole('button', /^Close$/).last().tap();
  await expect(button).toBeHidden(SLOW);
  await app.restart();
  await sleep(AFTER_CHECK_MS);
  await expect(screen.getByText(/^Start$/).last()).toBeVisible(SLOW);
  await expect(button).toBeHidden();
}

async function badCertIndex({ app, device, screen }: Context, platform: MobilePlatform): Promise<void> {
  await freshInstall(device);
  await app.open();
  await expect(screen.getByText('WELCOME TO BOREALIS')).toBeVisible(SLOW);
  await openAdditionalContentMobile(screen);
  await screen.getByRole('textbox').first().fill(BAD_CERT_INDEX);
  await device.dismissKeyboard();
  await screen.getByRole('button', /^Fetch$/).tap();
  await screen.getByRole('button', /^Add source$/).tap();
  // A clean, named failure (no "up to date", no content), and the app keeps running.
  await expect(statusText(screen, platform, /^Nothing was downloaded: 1 could not be updated\..*invalid peer certificate/)).toBeVisible(FAIL_TIMEOUT);
  await expect(statusText(screen, platform, /content pack\(s\) downloaded and loaded/)).toBeHidden();
  expect((await device.foregroundApp()).bundleId).toBe(BUNDLE_ID);
}

/** iOS: Share logs hands the newest log lines to the system share sheet (Android: see B-08 notes, no sheet yet). */
async function shareLogsIos({ app, device, screen }: Context): Promise<void> {
  await freshInstall(device);
  await app.open();
  await expect(screen.getByText('WELCOME TO BOREALIS')).toBeVisible(SLOW);
  await screen.getByRole('button', 'Settings').tap();
  const tab = screen.getByRole('button', 'Data files');
  await expect(tab).toBeVisible(SLOW);
  await tab.tap();
  await screen.getByRole('button', 'Share logs').tap();
  await expect(screen.getByText('Save to Files')).toBeVisible(SLOW);
}

const skip = process.env.E2E_B08 ? false : 'B-08 sweep only (E2E_B08=1, live network)';
for (const platform of ['ios', 'android'] as const) {
  const id = platform === 'ios' ? 'm5' : 'm5-android';
  describe(id, { tags: [platform, 'm5'], platforms: [platform], requires: ['native-app'] }, () => {
    test('update notice: button, flyout, Download opens the browser, Close is remembered', { timeout: 300_000, skip }, (ctx) => updateNotice(ctx, platform));
    if (platform === 'ios') test('Share logs opens the share sheet', { timeout: 300_000, skip }, (ctx) => shareLogsIos(ctx));
    test('an index on a host with a bad certificate fails with a message', { timeout: 300_000, skip }, (ctx) => badCertIndex(ctx, platform));
  });
}
