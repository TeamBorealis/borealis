import { expect } from 'e2e';
import { describe, test } from '@e2e-dev/mobile';
import type { Device } from '@e2e-dev/mobile';
import type { TestFixtures } from 'e2e';
import { freshInstall, SLOW, waitForShell } from '../../support/mobile.js';
import type { MobilePlatform, Shell } from '../../support/mobile.js';
import { openAdditionalContentMobile, scrollTo, statusText } from '../../support/mobile-content.js';
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
  html_url: `https://github.com/TeamBorealis/borealis/releases/tag/v${FIXTURE_VERSION}`,
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
/** Wide-shell Settings: the Data files intro (top of the scrolling body) and the most swipes tried to reach a row. */
const DATA_FILES_INTRO = 'Your data directories and additional content settings.';
const MAX_DIALOG_SWIPES = 8;
/** How far inside the Settings body a swipe starts and ends, so it never begins on the title or the footer. */
const DIALOG_SWIPE_INSET = 40;

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

/**
 * Wide shell (iPad): Settings is a fixed-height window over the page whose body scrolls. The tree lists rows below
 * the body's fold at their unscrolled place, where a tap lands outside the window, so swipe inside the body (from
 * above its footer up to below the Data files intro) until `target` sits above the footer.
 */
async function scrollIntoSettings(screen: TestFixtures['screen'], target: Locator): Promise<void> {
  const [top, footer] = await Promise.all([
    screen.getByText(DATA_FILES_INTRO).first().boundingBox(),
    screen.getByRole('button', /^Restore defaults$/i).boundingBox(),
  ]);
  if (!top || !footer) throw new Error('Settings window has no bounds to scroll within');
  const x = top.x + top.width / 2;
  const path = { from: { x, y: footer.y - DIALOG_SWIPE_INSET }, to: { x, y: top.y + top.height + DIALOG_SWIPE_INSET } };
  for (let i = 0; i < MAX_DIALOG_SWIPES; i += 1) {
    const box = await target.boundingBox().catch(() => null);
    if (box && box.y + box.height <= footer.y) return;
    await screen.swipe(path);
  }
  throw new Error('Share logs never scrolled into the Settings window');
}

/** Settings > Data files > Share logs (the tab is a button on iOS; on Android it is found by its text). */
async function tapShareLogs({ app, device, screen }: Context, platform: MobilePlatform): Promise<Shell> {
  await freshInstall(device);
  await app.open();
  await expect(screen.getByText(/^welcome to borealis$/i).first()).toBeVisible(SLOW);
  const shell = await waitForShell(screen);
  const tab = platform === 'ios' ? screen.getByRole('button', 'Data files') : screen.getByText('Data files').first();
  await tapUntilVisible(screen.getByRole('button', 'Settings'), tab);
  // The phone settings sheet keeps Share logs below the fold: open the tab, then scroll to it. The wide shell's
  // Settings window scrolls on its own (scrollIntoSettings).
  await tapUntilVisible(tab, screen.getByText(DATA_FILES_INTRO).first());
  const share = screen.getByRole('button', 'Share logs');
  if (shell === 'wide') await scrollIntoSettings(screen, share);
  else await scrollTo(screen, share);
  await share.tap();
  return shell;
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

/** Android (the sideloaded APK; iOS is a store build, see noUpdateNoticeIos). */
async function updateNotice({ app, device, screen }: Context): Promise<void> {
  await freshInstall(device);
  await app.open();
  await expect(screen.getByText(/^Start$/).last()).toBeVisible(SLOW);
  writeUpdateFeed('android', UPDATE_FEED);
  await app.restart();
  const button = screen.getByRole('button', UPDATE);
  await expect(button).toBeVisible(NOTICE_TIMEOUT);
  // Android's accessibility tree does not expose the flyout body (the "Release" region), only its
  // title and buttons (the version text was checked on screen, B-41).
  await tapUntilVisible(button, screen.getByRole('dialog', UPDATE));
  // Download hands the website to the system browser.
  await screen.getByRole('button', /^Download$/).tap();
  console.info(`[m5] Download opened ${await waitForeignForeground(device)}`);
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

/**
 * iOS is a store build (App Store rules, settings.storeBuild): even with a newer release in the feed fixture it never
 * shows the update notice.
 */
async function noUpdateNoticeIos({ app, device, screen }: Context): Promise<void> {
  await freshInstall(device);
  await app.open();
  // The phone bottom bar's Start tab; the wide shell (iPad) has the main tab strip's upper-case START link instead.
  const shell = await waitForShell(screen);
  const start = shell === 'wide' ? screen.getByRole('link', /^START$/) : screen.getByText(/^Start$/).last();
  await expect(start).toBeVisible(SLOW);
  writeUpdateFeed('ios', UPDATE_FEED);
  await app.restart();
  await expect(start).toBeVisible(SLOW);
  await sleep(AFTER_CHECK_MS);
  await expect(screen.getByRole('button', UPDATE)).toBeHidden();
}

async function badCertIndex(ctx: Context, platform: MobilePlatform): Promise<void> {
  const server = await startSelfSignedServer(platform, { [BAD_CERT_PATH]: BAD_CERT_BODY });
  try {
    await badCertIndexAt(ctx, platform, `${server.baseUrl}${BAD_CERT_PATH}`);
    // The failure came from this host: the app reached it and refused its certificate.
    expect(server.refusedHandshakes()).toBeGreaterThan(0);
  } finally {
    await server.close();
  }
}

async function badCertIndexAt({ app, device, screen }: Context, platform: MobilePlatform, url: string): Promise<void> {
  await freshInstall(device);
  await app.open();
  await expect(screen.getByText(/^welcome to borealis$/i).first()).toBeVisible(SLOW);
  const shell = await waitForShell(screen);
  await openAdditionalContentMobile(screen);
  // The wide shell's title bar holds the Compendium search box, the first textbox on screen: name the URL field.
  const field = shell === 'wide' ? screen.getByRole('textbox', 'Index URL') : screen.getByRole('textbox').first();
  await field.fill(url);
  await device.dismissKeyboard();
  await screen.getByRole('button', /^Fetch$/).tap();
  await screen.getByRole('button', /^Add source$/).tap();
  // A clean, named failure (no "up to date", no content), and the app keeps running. The status line names the
  // index and says it could not be downloaded; the transport text (the refused certificate) goes to the log only
  // (c236acc). The refused handshake on this host is checked by the caller.
  await expect(statusText(screen, platform, /^Nothing was downloaded: 1 could not be updated\. .*core\.index: Could not download the index file\./)).toBeVisible(FAIL_TIMEOUT);
  await expect(statusText(screen, platform, /content pack\(s\) downloaded and loaded/)).toBeHidden();
  expect((await device.foregroundApp()).bundleId).toBe(BUNDLE_ID);
}

/**
 * iOS: Share logs hands the newest log lines to the system share sheet (Web Share). On a phone the sheet's actions
 * are in the app's tree ("Save to Files"). On an iPad the sheet is a popover whose actions are drawn out of process:
 * the tree holds only its container, `ActivityListView` (seen on the iPad simulator while the popover showed
 * Copy and Save to Files on screen).
 */
async function shareLogsIos(ctx: Context): Promise<void> {
  const shell = await tapShareLogs(ctx, 'ios');
  const sheet = shell === 'wide' ? ctx.screen.getByTestId('ActivityListView') : ctx.screen.getByText('Save to Files');
  await expect(sheet).toBeVisible(SLOW);
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
    if (platform === 'ios') test('store build: no update notice even with a newer release in the feed', { timeout: 300_000, skip }, (ctx) => noUpdateNoticeIos(ctx));
    else test('update notice: button, flyout, Download opens the browser, Close is remembered', { timeout: 300_000, skip }, (ctx) => updateNotice(ctx));
    test('Share logs opens the share sheet', { timeout: 300_000, skip }, (ctx) => (platform === 'ios' ? shareLogsIos(ctx) : shareLogsAndroid(ctx)));
    test('an index on a host with a bad certificate fails with a message', { timeout: 300_000, skip }, (ctx) => badCertIndex(ctx, platform));
  });
}
