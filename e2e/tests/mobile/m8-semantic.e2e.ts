import { expect } from 'e2e';
import { describe, test } from '@e2e-dev/mobile';
import type { Device } from '@e2e-dev/mobile';
import type { TestFixtures } from 'e2e';
import { freshInstall, navLink, SLOW } from '../../support/mobile.js';
import type { MobilePlatform } from '../../support/mobile.js';

// m8 (S3.4, local-search-spec sections 7, 8; decision PH1) on the real phone builds: Settings > Search starts
// keyword-only; the consent dialog says indexing takes hours, runs while charging and that slower phones may stay
// keyword-only; Download and turn on fetches this device's model, the app loads it and times a query against the
// 40 ms budget. The device then decides: the too-slow line (keyword-only), indexing (or waiting for the charger), or
// "Keyword + semantic" once indexed. Whatever it decides, the line is one of these honest states.
// Opt-in (downloads 210-310 MB): E2E_M8=1 npx e2e run --target android --tag m8   (or --target ios)

type Context = TestFixtures & { device: Device };
type Screen = TestFixtures['screen'];
type Locator = ReturnType<Screen['getByText']>;

const PHONE_CONSENT = /Indexing can take several hours and runs while your device is charging\. On slower phones, search may stay keyword-only after the download\. Borealis checks the speed once the model has loaded\./;
const KEYWORD_ONLY = /^Downloads the EmbeddingGemma 2 model \((about 310|210) MB\) once\. Everything runs on this device\.$/;
const DECIDED = /^(Semantic search is too slow on this device: one search takes \d+ ms to embed \(the budget is 40 ms\), so search uses keywords\..*|Building the semantic index: \d+ of \d+ passages\..*|Adding new passages to the semantic index: .*|Indexing continues while .*|Indexing waits .*|Keyword \+ semantic: the semantic index is complete\.)$/;
const DOWNLOAD_AND_LOAD = { timeout: 15 * 60_000 } as const;
const RETAP_ATTEMPTS = 3;
const RETAP_WAIT_MS = 10_000;

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

async function semanticFlow(ctx: Context, platform: MobilePlatform): Promise<void> {
  const { screen, app } = ctx;
  await freshInstall(ctx.device);
  await app.open();
  await expect(screen.getByText(/^welcome to borealis$/i).first()).toBeVisible(SLOW);
  const semantic = screen.getByRole(platform === 'ios' ? 'switch' : 'checkbox', 'Semantic search (optional)');
  const row = screen.getByRole('button', 'Search settings');
  await tapUntilVisible(navLink(screen, platform, /^More$/), row);
  await tapUntilVisible(row, semantic);
  await expect(screen.getByText(KEYWORD_ONLY)).toBeVisible(SLOW);
  await app.screenshot(`m8-${platform}-1-keyword-only`);

  await semantic.tap();
  await expect(screen.getByText(PHONE_CONSENT)).toBeVisible(SLOW);
  await app.screenshot(`m8-${platform}-2-consent`);
  await screen.getByRole('button', 'Download and turn on').tap();
  await app.screenshot(`m8-${platform}-3-downloading`);

  const decided = screen.getByText(DECIDED).first();
  await expect(decided).toBeVisible(DOWNLOAD_AND_LOAD);
  const line = (await decided.textContent()) ?? '';
  process.stdout.write(`m8 ${platform} outcome: ${line}\n`);
  await app.screenshot(`m8-${platform}-4-outcome`);
  expect(line).toMatch(DECIDED);
}

const skip = process.env.E2E_M8 ? false : 'semantic download on the device builds only (E2E_M8=1)';
describe('m8', { tags: ['ios', 'm8'], platforms: ['ios'], requires: ['native-app'] }, () => {
  test('consent, download, then the per-device speed decision in Settings > Search', { timeout: 1_200_000, skip }, (ctx) => semanticFlow(ctx, 'ios'));
});
describe('m8-android', { tags: ['android', 'm8'], platforms: ['android'], requires: ['native-app'] }, () => {
  test('consent, download, then the per-device speed decision in Settings > Search', { timeout: 1_200_000, skip }, (ctx) => semanticFlow(ctx, 'android'));
});
