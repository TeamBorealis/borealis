import { expect } from 'e2e';
import { describe, test } from '@e2e-dev/mobile';
import type { Device } from '@e2e-dev/mobile';
import type { TestFixtures } from 'e2e';
import { freshInstall, SLOW } from '../../support/mobile.js';
import type { MobilePlatform } from '../../support/mobile.js';
import { openAdditionalContentMobile, statusText } from '../../support/mobile-content.js';
import { writeUpdateFeed } from '../../support/mobile-acceptance.js';
import { startSelfSignedServer } from '../../support/local-https.js';

// m5 (B-08 mobile sweep): the phone update notice (UP1/UP1a), Share logs (UP3) and an index download
// from a host with a bad TLS certificate (IM2). Opt-in: E2E_B08=1 npm run e2e:ios|e2e:android -- --tag m5
// No internet needed: the bad-certificate host is a local HTTPS server with a self-signed certificate
// made at test time (support/local-https.ts, B-42).
//
// Update notice test hook (B-41): a debug build answers the update check from a release-feed fixture in
// its data folder (src-tauri/src/app_update_debug.rs; release builds ignore it), so the notice shows on
// any app version and without GitHub. The fixture names a version no real release will reach.

const FIXTURE_VERSION = '99.0.0';
const UPDATE_FEED = JSON.stringify({
  tag_name: `v${FIXTURE_VERSION}`,
  html_url: `https://github.com/alon-mini/borealis/releases/tag/v${FIXTURE_VERSION}`,
  body: 'Fixture release notes (B-41).',
  draft: false,
  prerelease: false,
});
const UPDATE = 'Update Available';
/** With the fixture the backend answers at startup; allow for a slow first render. */
const NOTICE_TIMEOUT = { timeout: 60_000 } as const;
/** Long enough for the delayed phone check to have run and shown the button if it were going to. */
const AFTER_CHECK_MS = 25_000;
const FOREGROUND_POLL_MS = 1_000;
/** A tap that lands before the page is interactive is retried this often, waiting this long for its effect. */
const RETAP_ATTEMPTS = 3;
const RETAP_WAIT_MS = 10_000;
/** Long enough for the web view to have written localStorage to disk (15 s held by hand on Android). */
const STORAGE_FLUSH_MS = 15_000;
const FOREGROUND_TIMEOUT_MS = 20_000;
/** Served by a local host whose certificate is signed by nobody (self-signed, made at test time). */
const BAD_CERT_PATH = '/core.index';
const BAD_CERT_BODY = '<index/>\n';
const FAIL_TIMEOUT = { timeout: 120_000 } as const;
const BUNDLE_ID = 'app.borealis';

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
type Context = TestFixtures & { device: Device };

type Locator = ReturnType<TestFixtures['screen']['getByText']>;

/** Tap `target` until `expected` shows: a tap right after launch can land before the page is interactive (Android). */
async function tapUntilVisible(target: Locator, expected: Locator): Promise<void> {
  for (let attempt = 0; ; attempt += 1) {
    await target.tap();
    try {
      await expect(expected).toBeVisible({ timeout: RETAP_WAIT_MS });
      return;
    } catch (e) {
      if (attempt >= RETAP_ATTEMPTS - 1) throw e;
    }
  }
}

/** Settings > Data files > Share logs (the tab is a button on iOS; on Android it is found by its text). */
async function tapShareLogs({ app, device, screen }: Context, platform: MobilePlatform): Promise<void> {
  await freshInstall(device);
  await app.open();
  await expect(screen.getByText('WELCOME TO BOREALIS')).toBeVisible(SLOW);
  const tab = platform === 'ios' ? screen.getByRole('button', 'Data files') : screen.getByText('Data files').first();
  await tapUntilVisible(screen.getByRole('button', 'Settings'), tab);
  const share = screen.getByRole('button', 'Share logs');
  await tapUntilVisible(tab, share);
  await share.tap();
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
  await freshInstall(device);
  await app.open();
  await expect(screen.getByText(/^Start$/).last()).toBeVisible(SLOW);
  writeUpdateFeed(platform, UPDATE_FEED);
  await app.restart();
  const button = screen.getByRole('button', UPDATE);
  await expect(button).toBeVisible(NOTICE_TIMEOUT);
  // Android's accessibility tree does not expose the flyout body (the "Release" region), only its
  // title and buttons; the version text is checked on iOS (Android: checked on screen, B-41).
  const flyout = platform === 'ios'
    ? screen.getByText(new RegExp(`^Version ${FIXTURE_VERSION.replace(/\./g, '\\.')} of Borealis is available\\.`))
    : screen.getByText('UPDATE AVAILABLE').last();
  await tapUntilVisible(button, flyout);
  // Download hands the website to the system browser. iOS: agent-device reports the session app, not
  // Safari, so the browser hand-off is only checked on Android (B-08 checked iOS on screen).
  await screen.getByRole('button', /^Download$/).tap();
  if (platform === 'android') console.info(`[m5] Download opened ${await waitForeignForeground(device)}`);
  await app.open();
  await expect(button).toBeVisible(NOTICE_TIMEOUT);
  const close = screen.getByRole('button', /^Close$/).last();
  await tapUntilVisible(button, close);
  // Close dismisses this version; the button is gone and stays gone after a restart.
  await close.tap();
  await expect(button).toBeHidden(SLOW);
  // The dismissal lives in localStorage, which the Android WebView writes to disk lazily: a restart
  // right after Close loses it (seen in B-41). Give it time to land first.
  await sleep(STORAGE_FLUSH_MS);
  await app.restart();
  await sleep(AFTER_CHECK_MS);
  await expect(screen.getByText(/^Start$/).last()).toBeVisible(SLOW);
  await expect(button).toBeHidden();
}

async function badCertIndex(ctx: Context, platform: MobilePlatform): Promise<void> {
  const server = await startSelfSignedServer(platform, { [BAD_CERT_PATH]: BAD_CERT_BODY });
  try {
    await badCertIndexAt(ctx, platform, `${server.baseUrl}${BAD_CERT_PATH}`);
  } finally {
    await server.close();
  }
}

async function badCertIndexAt({ app, device, screen }: Context, platform: MobilePlatform, url: string): Promise<void> {
  await freshInstall(device);
  await app.open();
  await expect(screen.getByText('WELCOME TO BOREALIS')).toBeVisible(SLOW);
  await openAdditionalContentMobile(screen);
  await screen.getByRole('textbox').first().fill(url);
  await device.dismissKeyboard();
  await screen.getByRole('button', /^Fetch$/).tap();
  await screen.getByRole('button', /^Add source$/).tap();
  // A clean, named failure (no "up to date", no content), and the app keeps running.
  await expect(statusText(screen, platform, /^Nothing was downloaded: 1 could not be updated\..*invalid peer certificate/)).toBeVisible(FAIL_TIMEOUT);
  await expect(statusText(screen, platform, /content pack\(s\) downloaded and loaded/)).toBeHidden();
  expect((await device.foregroundApp()).bundleId).toBe(BUNDLE_ID);
}

/** iOS: Share logs hands the newest log lines to the system share sheet (Web Share). */
async function shareLogsIos(ctx: Context): Promise<void> {
  await tapShareLogs(ctx, 'ios');
  await expect(ctx.screen.getByText('Save to Files')).toBeVisible(SLOW);
}

/** Android (B-40): Share logs opens the system share sheet (native ACTION_SEND), not the clipboard fallback. */
async function shareLogsAndroid(ctx: Context): Promise<void> {
  const { device, screen } = ctx;
  await tapShareLogs(ctx, 'android');
  console.info(`[m5] Share logs opened ${await waitForeignForeground(device)}`);
  await device.back();
  await expect(screen.getByText(/copied to the clipboard/)).toBeHidden();
}

const skip = process.env.E2E_B08 ? false : 'B-08 sweep only (E2E_B08=1)';
for (const platform of ['ios', 'android'] as const) {
  const id = platform === 'ios' ? 'm5' : 'm5-android';
  describe(id, { tags: [platform, 'm5'], platforms: [platform], requires: ['native-app'] }, () => {
    test('update notice: button, flyout, Download opens the browser, Close is remembered', { timeout: 300_000, skip }, (ctx) => updateNotice(ctx, platform));
    test('Share logs opens the share sheet', { timeout: 300_000, skip }, (ctx) => (platform === 'ios' ? shareLogsIos(ctx) : shareLogsAndroid(ctx)));
    test('an index on a host with a bad certificate fails with a message', { timeout: 300_000, skip }, (ctx) => badCertIndex(ctx, platform));
  });
}
