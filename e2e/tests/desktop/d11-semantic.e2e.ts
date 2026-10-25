import { desktopSuite, expect, pressKey, SLOW } from './helpers.js';

// d11 (S3.4, local-search-spec sections 7, 8, 12): the semantic search flow in the real macOS app. Fresh data; the
// switch line says keyword-only; the consent dialog (desktop wording) comes first; Download and turn on fetches the
// model (about 310 MB, the pinned Hugging Face / mirror URLs), the app loads it, times a query against the 40 ms
// budget and builds the semantic index; Settings > Search then says "Keyword + semantic" and so does the search
// footer. A machine over the budget shows the too-slow line instead (honest keyword-only, decision PH1).
// Opt-in (network; indexing about 20-60 minutes depending on the machine and its load):
// E2E_TELEMETRY_DISABLED=1 E2E_D11=1 npx e2e run --target macos --tag d11 (not `npm run e2e:macos`, whose `--tag macos` adds every
// desktop suite)
const SWITCH = /^Semantic search \(optional\)$/;
const KEYWORD_ONLY = /^Downloads the EmbeddingGemma 2 model \(about 310 MB\) once\. Everything runs on this device\.$/;
const READY = 'Keyword + semantic: the semantic index is complete.';
const TOO_SLOW = /^Semantic search is too slow on this device: one search takes \d+ ms to embed \(the budget is 40 ms\), so search uses keywords\./;
const OUTCOME = /^(Keyword \+ semantic: the semantic index is complete\.|Semantic search is too slow on this device: .*)$/;
const INDEX_WAIT = { timeout: 100 * 60_000 } as const;

const skip = process.env.E2E_D11 ? undefined : 'semantic download + indexing on the real app only (E2E_D11=1)';

desktopSuite('d11', 'consent, download, index, then Keyword + semantic in Settings > Search', async (ctx) => {
  const { app, screen } = ctx;
  await app.open();
  await expect(screen.getByText(/welcome to borealis/i).first()).toBeVisible(SLOW);
  await screen.getByRole('menuitem', 'View').tap();
  await screen.getByRole('menuitem', 'Borealis Settings').tap();
  // The macOS tree reports the settings tabs without the tab role: match the exact name.
  await screen.getByLabel(/^Search$/).first().tap();
  await expect(screen.getByText(KEYWORD_ONLY).first()).toBeVisible(SLOW);
  await app.screenshot('d11-1-keyword-only');

  await screen.getByRole('checkbox', SWITCH).tap();
  await expect(screen.getByText(/^Turn on semantic search\?$/).first()).toBeVisible(SLOW);
  await expect(screen.getByText(/Indexing takes a few minutes\./).first()).toBeVisible(SLOW);
  await app.screenshot('d11-2-consent');
  await screen.getByRole('button', 'Download and turn on').tap();

  await expect(screen.getByText(/^Downloading the model: |^Checking the downloaded file|^Loading the semantic search model|^Building the semantic index: /).first()).toBeVisible(SLOW);
  await app.screenshot('d11-3-downloading');
  await expect(screen.getByText(/^Building the semantic index: \d+ of \d+ passages\./).first()).toBeVisible({ timeout: 15 * 60_000 })
    .catch(() => undefined); // a too-slow machine never builds it; the outcome below decides
  await app.screenshot('d11-4-indexing');

  const outcome = screen.getByText(OUTCOME).first();
  await expect(outcome).toBeVisible(INDEX_WAIT);
  const line = (await outcome.textContent()) ?? '';
  await app.screenshot('d11-5-outcome');
  process.stdout.write(`d11 outcome: ${line}\n`);
  if (TOO_SLOW.test(line)) return;
  expect(line).toBe(READY);
  await expect(screen.getByText(/^On: search also finds rules, lore and session notes by meaning\.$/).first()).toBeVisible(SLOW);

  await screen.getByRole('button', 'Close').last().tap();
  await screen.getByLabel('Compendium search').fill('spell that makes an enemy fall asleep');
  await pressKey(ctx, 'Enter');
  await expect(screen.getByLabel(/^Search mode/).first()).toBeVisible(SLOW);
  await expect(screen.getByText(/^Keyword \+ semantic$/).first()).toBeVisible(SLOW);
  await app.screenshot('d11-6-footer');
}, { timeout: 120 * 60_000, ...(skip ? { skip } : {}) });
