import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdtempSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { expect } from 'e2e';
import { describe, test } from '@e2e-dev/mobile';
import type { Device } from '@e2e-dev/mobile';
import type { TestFixtures } from 'e2e';
import { characterEntry, freshInstall, SLOW } from '../../support/mobile.js';
import { ANDROID_BUNDLE_ID } from '../../support/mobile-env.js';

// m6 (B-24): a .dnd5e character and a .index content index opened with Borealis from outside the app,
// with the app running (warm) and not running (cold). Opt-in: E2E_B24=1 npm run e2e:android -- --tag m6
//
// Android only: the file is pushed to Downloads, found in the media store, and opened with a VIEW intent
// carrying a read grant, which is how a file manager hands it over (an adb shell can grant a media-store
// URI, not a Files/Downloads document URI). The intent names the activity so no chooser appears; the
// manifest match itself is checked with `cmd package query-activities`. The Downloads document URIs that
// Files uses, and iOS Files > Share > Borealis, were run by hand (specs b24d).

const ADB = join(process.env.ANDROID_HOME ?? join(homedir(), 'Library/Android/sdk'), 'platform-tools/adb');
const adb = (...args: string[]) => execFileSync(ADB, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
const ACTIVITY = `${ANDROID_BUNDLE_ID}/.MainActivity`;
/** What Android reports for a file type it does not know (.dnd5e, .index). */
const OCTET = 'application/octet-stream';
const HERO = 'Test Hero';
const HERO_FIXTURE = join(import.meta.dirname, '../../fixtures/d5-hero.dnd5e');
/** A synthetic index whose one listed file is not present: the preview opens and says nothing can be imported. */
const INDEX_XML = '<?xml version="1.0" encoding="utf-8"?>\n<index>\n  <files>\n    <file name="m6-listed.xml" url="https://example.invalid/m6-listed.xml"/>\n  </files>\n</index>\n';
const SCAN_ATTEMPTS = 10;
const SCAN_PAUSE_MS = 1_000;

const RETAP_ATTEMPTS = 3;
const RETAP_WAIT_MS = 10_000;

type Context = TestFixtures & { device: Device };
type Locator = ReturnType<TestFixtures['screen']['getByText']>;
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** Synthetic inputs, named per run so the media store never answers with an older copy. */
function makeInputs(): { hero: string; index: string } {
  const dir = mkdtempSync(join(tmpdir(), 'borealis-m6-'));
  const tag = Date.now().toString(36);
  const hero = join(dir, `m6-hero-${tag}.dnd5e`);
  const index = join(dir, `m6-index-${tag}.index`);
  copyFileSync(HERO_FIXTURE, hero);
  writeFileSync(index, INDEX_XML);
  return { hero, index };
}

/** Push to Downloads, have the media store index it, and answer its content URI. */
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

/** The manifest offers Borealis for the URI without naming the activity (what a file manager asks). */
function expectOffered(uri: string): void {
  const out = adb('shell', 'cmd', 'package', 'query-activities', '-a', 'android.intent.action.VIEW', '-d', uri, '-t', OCTET);
  expect(out).toContain(`name=${ANDROID_BUNDLE_ID}.MainActivity`);
}

function openWith(uri: string): void {
  adb('shell', 'am', 'start', '-W', '-n', ACTIVITY, '-a', 'android.intent.action.VIEW', '-d', uri, '-t', OCTET, '--grant-read-uri-permission');
}

/** Install fresh, check the manifest offers Borealis, then open. Warm: the app is up on its start page. Cold: the intent starts it. */
async function start(ctx: Context, warm: boolean, uri: string): Promise<void> {
  await freshInstall(ctx.device);
  expectOffered(uri);
  if (warm) {
    await ctx.app.open();
    await expect(ctx.screen.getByText('WELCOME TO BOREALIS')).toBeVisible(SLOW);
  } else {
    adb('shell', 'am', 'force-stop', ANDROID_BUNDLE_ID);
  }
  openWith(uri);
  // Observe the app the intent started: a foreground-only open (`app.open()` would relaunch it and drop the file).
  if (!warm) await ctx.device.openApp(ANDROID_BUNDLE_ID);
}

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

async function heroOpened(ctx: Context, warm: boolean): Promise<void> {
  const uri = await mediaStoreUri(makeInputs().hero);
  await start(ctx, warm, uri);
  // Imported and listed on the Characters page the app is showing (no reload needed).
  await expect(characterEntry(ctx.screen, HERO)).toBeVisible(SLOW);
}

async function indexOpened(ctx: Context, warm: boolean): Promise<void> {
  const uri = await mediaStoreUri(makeInputs().index);
  await start(ctx, warm, uri);
  // The content importer takes it: first-import notice, then the preview of this index.
  const accept = ctx.screen.getByRole('button', /^I understand, continue$/i);
  await expect(accept).toBeVisible(SLOW);
  await tapUntilVisible(accept, ctx.screen.getByRole('dialog', /^Import preview$/));
  await expect(ctx.screen.getByText(/^Nothing in m6-index-\w+ can be imported\.$/).first()).toBeVisible(SLOW);
}

const skip = process.env.E2E_B24 ? false : 'B-24 open-with sweep only (E2E_B24=1)';
describe('m6-android', { tags: ['android', 'm6'], platforms: ['android'], requires: ['native-app'] }, () => {
  test('a .dnd5e opened while the app runs is imported and listed', { timeout: 300_000, skip }, (ctx) => heroOpened(ctx, true));
  test('a .dnd5e that starts the app is imported and listed', { timeout: 300_000, skip }, (ctx) => heroOpened(ctx, false));
  test('a .index opened while the app runs opens the import preview', { timeout: 300_000, skip }, (ctx) => indexOpened(ctx, true));
  test('a .index that starts the app opens the import preview', { timeout: 300_000, skip }, (ctx) => indexOpened(ctx, false));
});
