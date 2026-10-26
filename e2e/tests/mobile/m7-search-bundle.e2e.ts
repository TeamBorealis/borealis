import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdtempSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { expect } from 'e2e';
import { describe, test } from '@e2e-dev/mobile';
import type { Device } from '@e2e-dev/mobile';
import type { TestFixtures } from 'e2e';
import { characterEntry, freshInstall, navLink, SLOW } from '../../support/mobile.js';
import type { MobilePlatform } from '../../support/mobile.js';
import { ANDROID_BUNDLE_ID } from '../../support/mobile-env.js';

// m7 (B-12 search, B-14 bundles) on the real phone builds: the keyword index builds on the device and
// `firball` puts Fireball first; the Rules scope opens a section with its source and attribution;
// Settings > Search offers semantic search (the llama.cpp backend is linked since B-12; nothing downloads without
// the consent dialog) and shows the device tier.
// Android also opens a `.borealis` file from outside the app (warm and cold): preview, Accept, listed.
// Opt-in: E2E_M7=1 npm run e2e:android -- --tag m7   (or e2e:ios; iOS runs the search checks only:
// a bundle reaches the app there through Files > Share, which is not in the accessibility tree).

type Context = TestFixtures & { device: Device };
type Screen = TestFixtures['screen'];

const ADB = join(process.env.ANDROID_HOME ?? join(homedir(), 'Library/Android/sdk'), 'platform-tools/adb');
const adb = (...args: string[]) => execFileSync(ADB, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
const ACTIVITY = `${ANDROID_BUNDLE_ID}/.MainActivity`;
const OCTET = 'application/octet-stream';
/** Synthetic bundle: one character, "Test Hero" (the d5-hero fixture), made by Share as a file. */
const BUNDLE_FIXTURE = join(import.meta.dirname, '../../fixtures/m7-hero.borealis');
const HERO = 'Test Hero';
const SCAN_ATTEMPTS = 10;
const SCAN_PAUSE_MS = 1_000;
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

type Locator = ReturnType<Screen['getByText']>;
const RETAP_ATTEMPTS = 3;
const RETAP_WAIT_MS = 10_000;

/** Tap `target` until `expected` shows: a tap during the start-up splash is lost (as in m6). */
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

/** More tab -> one of its rows, until `opened` shows. */
async function moreRow(screen: Screen, platform: MobilePlatform, row: Locator, opened: Locator): Promise<void> {
  await tapUntilVisible(navLink(screen, platform, /^More$/), row);
  await tapUntilVisible(row, opened);
}

async function searchChecks(ctx: Context, platform: MobilePlatform): Promise<void> {
  const { screen } = ctx;
  await freshInstall(ctx.device);
  await ctx.app.open();
  await expect(screen.getByText(/^welcome to borealis$/i).first()).toBeVisible(SLOW);

  // Roles differ per platform (iOS reports the dialog and the result lists as "other"), so these
  // checks go by accessible name.
  const box = screen.getByRole('textbox');
  await moreRow(screen, platform, navLink(screen, platform, /^Search$/), box);
  await box.fill('firball');
  // The first group is Spells and its first row is Fireball (one of the two editions).
  const firstGroup = screen.getByLabel(/^Compendium results: /).first();
  await expect(firstGroup).toHaveAccessibleName('Compendium results: Spells', SLOW);
  const firstRow = await firstGroup.getByRole('button').first().textContent();
  expect(firstRow ?? '').toMatch(/^Fireball(SRD|2014)/);
  // The footer note (Android exposes it as a group named "Search mode" without its text).
  await expect(screen.getByLabel(/^Search mode/)).toBeVisible();

  await screen.getByLabel('Search scope').getByLabel('Rules', { exact: true }).tap();
  await box.fill('grappled');
  await screen.getByLabel('Compendium results: Rules').getByRole('button').first().tap();
  await expect(screen.getByRole('button', 'Back to results')).toBeVisible(SLOW);
  await expect(screen.getByText(/^source$/i).first()).toBeVisible();
  await expect(screen.getByText(/System Reference Document 5\.[12]/).first()).toBeVisible();
  await ctx.app.open();

  await expect(screen.getByText(/^welcome to borealis$/i).first()).toBeVisible(SLOW);
  // iOS reports the settings switch as a switch and the tier combo's choice as its value.
  const semantic = screen.getByRole(platform === 'ios' ? 'switch' : 'checkbox', 'Semantic search (optional)');
  await moreRow(screen, platform, screen.getByRole('button', 'Search settings'), semantic);
  await expect(semantic).toBeEnabled();
  await expect(semantic).not.toBeChecked();
  await expect(screen.getByText(/^Downloads the EmbeddingGemma 2 model \((about 310|210) MB\) once\./)).toBeVisible();
  await expect(screen.getByText(/^device tier$/i).first()).toBeVisible();
  const tier = /^Automatic \(Tier [123]: /;
  await expect(platform === 'ios' ? screen.getByDisplayValue(tier) : screen.getByLabel(tier)).toBeVisible();
}

/** Push to Downloads, have the media store index it, and answer its content URI (as m6). */
async function mediaStoreUri(file: string): Promise<string> {
  const name = basename(file);
  adb('push', file, `/sdcard/Download/${name}`);
  adb('shell', 'am', 'broadcast', '-a', 'android.intent.action.MEDIA_SCANNER_SCAN_FILE', '-d', `file:///sdcard/Download/${name}`);
  for (let i = 0; i < SCAN_ATTEMPTS; i += 1) {
    const row = adb('shell', 'content', 'query', '--uri', 'content://media/external/file', '--projection', '_id', '--where', `"_display_name='${name}'"`);
    const id = /_id=(\d+)/.exec(row)?.[1];
    if (id) return `content://media/external/file/${id}`;
    await sleep(SCAN_PAUSE_MS);
  }
  throw new Error(`${name} never reached the media store`);
}

async function bundleOpened(ctx: Context, warm: boolean): Promise<void> {
  const { screen } = ctx;
  const name = `m7-hero-${Date.now().toString(36)}.borealis`;
  const file = join(mkdtempSync(join(tmpdir(), 'borealis-m7-')), name);
  copyFileSync(BUNDLE_FIXTURE, file);
  const uri = await mediaStoreUri(file);
  await freshInstall(ctx.device);
  const offered = adb('shell', 'cmd', 'package', 'query-activities', '-a', 'android.intent.action.VIEW', '-d', uri, '-t', OCTET);
  expect(offered).toContain(`name=${ANDROID_BUNDLE_ID}.MainActivity`);
  if (warm) {
    await ctx.app.open();
    await expect(screen.getByText(/^welcome to borealis$/i).first()).toBeVisible(SLOW);
  } else {
    adb('shell', 'am', 'force-stop', ANDROID_BUNDLE_ID);
  }
  adb('shell', 'am', 'start', '-W', '-n', ACTIVITY, '-a', 'android.intent.action.VIEW', '-d', uri, '-t', OCTET, '--grant-read-uri-permission');
  if (!warm) await ctx.device.openApp(ANDROID_BUNDLE_ID);

  await expect(screen.getByRole('dialog', 'Open bundle')).toBeVisible(SLOW);
  // The preview names the file the user opened, not the app's temporary copy.
  await expect(screen.getByLabel(name)).toBeVisible(SLOW);
  await expect(screen.getByRole('list', 'What Accept changes')).toBeVisible();
  await screen.getByRole('button', 'Accept').tap();
  await expect(screen.getByLabel(`Opened ${name}: 1 new.`)).toBeVisible(SLOW);
  await expect(characterEntry(screen, HERO)).toBeVisible(SLOW);
}

const skip = process.env.E2E_M7 ? false : 'device search + bundle checks only (E2E_M7=1)';
describe('m7', { tags: ['ios', 'm7'], platforms: ['ios'], requires: ['native-app'] }, () => {
  test('firball finds Fireball first, Rules opens a sourced section, Settings > Search shows the tier', { timeout: 300_000, skip }, (ctx) => searchChecks(ctx, 'ios'));
});
describe('m7-android', { tags: ['android', 'm7'], platforms: ['android'], requires: ['native-app'] }, () => {
  test('firball finds Fireball first, Rules opens a sourced section, Settings > Search shows the tier', { timeout: 300_000, skip }, (ctx) => searchChecks(ctx, 'android'));
  test('a .borealis opened while the app runs shows its preview and Accept lists the character', { timeout: 300_000, skip }, (ctx) => bundleOpened(ctx, true));
  test('a .borealis that starts the app shows its preview and Accept lists the character', { timeout: 300_000, skip }, (ctx) => bundleOpened(ctx, false));
});
