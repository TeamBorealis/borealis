import { existsSync, readFileSync } from 'node:fs';
import { basename } from 'node:path';
import { expect } from 'e2e';
import { describe, test } from '@e2e-dev/mobile';
import type { Device } from '@e2e-dev/mobile';
import type { TestFixtures } from 'e2e';
import { freshInstall, SLOW } from '../../support/mobile.js';
import type { MobilePlatform } from '../../support/mobile.js';
import { checkThresholds, deliverInputs, peakMemoryKb, readDigest, requestDigest, writeResult } from '../../support/mobile-acceptance.js';
import { chooseFile, openAdditionalContentMobile, scrollTo, statusText } from '../../support/mobile-content.js';

// m4 (S5, specs/acceptance-imports-1.md): the phone imports the acceptance inputs through the UI and its
// debug-only `acceptance_digest` must equal the macOS reference for the same inputs
// (src-tauri/tests/acceptance_digest_ref.rs). Opt-in (network, private inputs; 4-7 min per platform):
//   E2E_S5=1 E2E_S5_A_USER_ZIP=<a-user.zip> E2E_S5_F_FULL_ZIP=<f-full.zip> E2E_S5_REFERENCE=<mac digest json>
//   E2E_S5_OUT=<dir for m4-<platform>.json>  [E2E_S5_INDEX_URL, default A-core's core.index]
// Steps: Update indexes with the index URL, import the A-user zip and the F-full zip with the file chooser,
// restart, read the digest. Records the time of each import and the app's peak memory.

const INDEX_URL = process.env.E2E_S5_INDEX_URL ?? 'https://raw.githubusercontent.com/AuroraLegacy/elements/master/core.index';
const IMPORT_TIMEOUT = 900_000;
/** Whole run: three imports, a restart and the digest on a debug build (4-7 min measured; generous cap). */
const TEST_TIMEOUT = 3_600_000;
const DIGEST_TIMEOUT_MS = 900_000;
const POLL_MS = 5_000;

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

function input(name: string): string {
  const path = process.env[name];
  if (!path || !existsSync(path)) throw new Error(`${name} must name an existing file`);
  return path;
}

type Context = TestFixtures & { device: Device };

async function m4({ app, device, screen }: Context, platform: MobilePlatform): Promise<void> {
  const aUser = input('E2E_S5_A_USER_ZIP');
  const fFull = input('E2E_S5_F_FULL_ZIP');
  const reference = JSON.parse(readFileSync(input('E2E_S5_REFERENCE'), 'utf8')) as Record<string, unknown>;
  const times: Record<string, number> = {};
  const peakKb: Record<string, number | undefined> = {};

  await freshInstall(device);
  deliverInputs(platform, [aUser, fFull]);
  await app.open();
  await expect(screen.getByText('WELCOME TO BOREALIS')).toBeVisible(SLOW);
  await openAdditionalContentMobile(screen);

  // 1) A-core: Fetch the index URL (consent), which downloads it right away.
  const indexField = screen.getByRole('textbox').first();
  await indexField.fill(INDEX_URL);
  await device.dismissKeyboard();
  await screen.getByRole('button', /^Fetch$/).tap();
  await screen.getByRole('button', /^Add source$/).tap();
  let t = Date.now();
  await expect(statusText(screen, platform, /content pack\(s\) downloaded and loaded\.$/)).toBeVisible({ timeout: IMPORT_TIMEOUT });
  times.aCoreMs = Date.now() - t;
  peakKb.afterACore = peakMemoryKb(platform);

  // 2) A-user: Import files -> notice -> chooser -> preview -> Install -> report.
  const importFiles = screen.getByRole('button', /^Import files$/);
  await scrollTo(screen, importFiles);
  await importFiles.tap();
  await screen.getByRole('button', 'I understand, continue').tap();
  t = Date.now();
  await chooseFile(screen, device, platform, basename(aUser));
  await screen.getByRole('button', /^Install$/).tap();
  await expect(statusText(screen, platform, /^\d+ elements converted; /)).toBeVisible({ timeout: IMPORT_TIMEOUT });
  times.aUserMs = Date.now() - t;
  peakKb.afterAUser = peakMemoryKb(platform);
  await screen.getByRole('button', /^close$/i).last().tap();

  // 3) F-full: 5etools zip -> notice -> chooser -> every book -> Import (the button is disabled while it runs).
  const zipButton = screen.getByRole('button', /^Choose a \.zip/);
  await scrollTo(screen, zipButton);
  await zipButton.tap();
  await screen.getByRole('button', 'I understand, continue').tap();
  t = Date.now();
  await chooseFile(screen, device, platform, basename(fFull));
  const importButton = screen.getByRole('button', /^Import \d+ book/);
  await scrollTo(screen, importButton);
  times.fFullStageMs = Date.now() - t;
  await importButton.tap();
  await expect(importButton).toBeDisabled({ timeout: 60_000 });
  await expect(importButton).toBeEnabled({ timeout: IMPORT_TIMEOUT });
  times.fFullMs = Date.now() - t;
  peakKb.afterFFull = peakMemoryKb(platform);

  // 4) restart; the debug build answers the digest request once the content is loaded.
  requestDigest(platform);
  t = Date.now();
  await app.restart();
  let digest = readDigest(platform);
  while (!digest && Date.now() - t < DIGEST_TIMEOUT_MS) {
    await sleep(POLL_MS);
    digest = readDigest(platform);
  }
  times.restartToDigestMs = Date.now() - t;
  peakKb.afterDigest = peakMemoryKb(platform);
  const out = process.env.E2E_S5_OUT ?? '.e2e/s5';
  const file = writeResult(out, platform, { platform, times, peakKb, digest });
  console.info(`[m4] ${platform}: ${JSON.stringify({ times, peakKb })} -> ${file}`);
  if (!digest) throw new Error('the app wrote no acceptance digest');
  expect(digest.error).toBeUndefined();
  expect(digest.counts).toEqual(reference.counts);
  expect(digest.contentDigest).toBe(reference.contentDigest);
  expect(digest.characters).toEqual(reference.characters);
  expect(digest.digest).toBe(reference.digest);
  // Time and peak memory limits (tools/acceptance/thresholds.json), checked after the digest equality.
  const limits = checkThresholds(platform, times, peakKb, out);
  console.info(limits.output);
  if (limits.code !== 0) {
    throw new Error(`S5 ${platform}: a time or peak-memory limit is exceeded or could not be checked (check_thresholds exit ${limits.code}); numbers are in the log above`);
  }
}

for (const platform of ['ios', 'android'] as const) {
  const id = platform === 'ios' ? 'm4' : 'm4-android';
  describe(id, { tags: [platform, 'm4'], platforms: [platform], requires: ['native-app'] }, () => {
    test('imports A-core, A-user and F-full and matches the macOS acceptance digest', { timeout: TEST_TIMEOUT, skip: process.env.E2E_S5 ? false : 'S5 acceptance only (E2E_S5=1)' }, (context) => m4(context, platform));
  });
}
