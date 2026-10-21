import { desktopSuite, expect, openAdditionalContent, SLOW, tapInView } from './helpers.js';

// d10 (B-07): real backend and network, Additional content > Index files with a remote Aurora .index URL
// (BOREALIS_LIVE_INDEX_URL, e.g. https://raw.githubusercontent.com/AuroraLegacy/elements/master/core.index).
// Fetch asks for consent first, naming the host: Cancel adds nothing; Add source lists the index with the
// "unverified" chip (not signed by the pinned key) and downloads it right away. Skipped when the URL is unset.
const URL_ENV = 'BOREALIS_LIVE_INDEX_URL';
const LIVE_URL = process.env[URL_ENV];
const CONSENT = /^This source is not verified by Borealis\. Content will be downloaded from /;
/** The whole core tree is downloaded and converted on the first fetch. */
const DOWNLOAD_TIMEOUT_MS = 600_000;

desktopSuite('d10', 'Index URL: consent names the host, cancel adds nothing, the added index is unverified and downloads', async (ctx) => {
  const { app, screen } = ctx;
  if (LIVE_URL === undefined) throw new Error(`${URL_ENV} is not set`); // skipped at collection
  const host = new URL(LIVE_URL).host;
  await app.open();
  await openAdditionalContent(ctx);
  await expect(screen.getByText('No index files downloaded.')).toBeVisible(SLOW);
  const field = screen.getByRole('textbox', 'Index URL');

  // Cancel: the consent names the host and nothing is added.
  await tapInView(ctx, field);
  await field.fill(LIVE_URL);
  await screen.getByRole('button', 'Fetch').tap();
  await expect(screen.getByText('Add content source').first()).toBeVisible(SLOW);
  await expect(screen.getByText(CONSENT).first()).toBeVisible(SLOW);
  await expect(screen.getByText(new RegExp(`downloaded from ${host.replace(/\./g, '\\.')} and converted`)).first()).toBeVisible(SLOW);
  await screen.getByRole('button', 'Cancel').tap();
  await expect(screen.getByText(CONSENT)).toBeHidden(SLOW);
  await expect(screen.getByText('No index files downloaded.')).toBeVisible(SLOW);

  // Add source: listed with the unverified chip, then downloaded by the same Fetch.
  await screen.getByRole('button', 'Fetch').tap();
  await expect(screen.getByText(CONSENT).first()).toBeVisible(SLOW);
  await screen.getByRole('button', 'Add source').tap();
  await expect(screen.getByText(new RegExp(`^${LIVE_URL.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&')} · `)).first()).toBeVisible(SLOW);
  await expect(screen.getByText(/^unverified$/i).first()).toBeVisible(SLOW);
  const start = Date.now();
  await expect(screen.getByText(/content pack\(s\) downloaded and loaded/).first()).toBeVisible({ timeout: DOWNLOAD_TIMEOUT_MS });
  console.info(`[d10] add source to downloaded: ${Date.now() - start} ms`);
  await expect(screen.getByText(/could not be updated|Nothing was downloaded/)).toBeHidden({ timeout: 3000 });
  await expect(screen.getByText(/^unverified$/i).first()).toBeVisible(SLOW);
}, { timeout: DOWNLOAD_TIMEOUT_MS + 120_000, ...(LIVE_URL === undefined ? { skip: `${URL_ENV} is not set` } : {}) });
