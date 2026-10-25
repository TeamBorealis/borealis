import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import type { Ctx } from '../support/flows-a.js';
import { pressKey } from '../support/flows-b.js';

// S3.4 (local-search-spec.md sections 7, 8, 12; decision PH1): the Settings > Search lines of the semantic index in
// the mock, which mirrors the app's states: keyword-only before the download, the download progress, the index being
// built, keyword + semantic once it is complete, and keyword-only for speed when the device embeds a query slower
// than the 40 ms budget (with the phone consent that says so). No request leaves the page.
const SWITCH = 'Semantic search (optional)';
const KEYWORD_ONLY = 'Downloads the EmbeddingGemma 2 model (about 310 MB) once. Everything runs on this device.';
const ON_BUILDING = 'On: search uses keywords until the semantic index is built.';
const ON_READY = 'On: search also finds rules, lore and session notes by meaning.';
const READY = 'Keyword + semantic: the semantic index is complete.';
const ON_KEYWORDS = 'On: search uses keywords on this device for now.';
const BUILDING = /^Building the semantic index: \d+ of 1200 passages\. Search uses keywords until it is done\.$/;
const DOWNLOADING = /^Downloading the model: [\d.]+ of [\d.]+ MB\.$/;
const PHONE_INDEXING = 'Indexing can take several hours and runs while your device is charging. On slower phones, search may stay keyword-only after the download. Borealis checks the speed once the model has loaded.';
const TOO_SLOW = 'Semantic search is too slow on this device: one search takes 95 ms to embed (the budget is 40 ms), so search uses keywords. You can delete the model in Settings.';
const WAIT_MS = 15_000;

/** Every request that is not the local dev server is recorded and aborted. */
async function blockExternal(ctx: Ctx): Promise<string[]> {
  const external: string[] = [];
  await ctx.browser.route(/.*/, async (route) => {
    const url = route.request.url;
    if (/^https?:\/\/(127\.0\.0\.1|localhost)(:\d+)?\//.test(url) || url.startsWith('data:')) await route.continue();
    else { external.push(url); await route.abort(); }
  });
  return external;
}

async function openSearchSettings(ctx: Ctx): Promise<void> {
  await ctx.screen.getByRole('menuitem', 'View').tap();
  await ctx.screen.getByRole('menuitem', 'Borealis Settings').tap();
  await expect(ctx.screen.getByRole('dialog', 'Borealis Settings')).toBeVisible();
  await ctx.screen.getByRole('tab', 'Search').tap();
  await expect(ctx.screen.getByRole('checkbox', SWITCH)).toBeVisible();
}

/** innerText of every Settings > Search help and status line, in page order. */
const lines = (ctx: Ctx): Promise<string[]> => ctx.browser.evaluate((_: null) =>
  Array.from(document.querySelectorAll('.aur-set-win .aur-set-help, .aur-set-win [role=status], .aur-set-win .aur-set-check-desc'))
    .map((e) => (e as HTMLElement).innerText.trim()).filter((t) => t.length > 0), null);

/** Poll until one Settings line matches `want`; returns it. */
async function waitLine(ctx: Ctx, label: string, want: string | RegExp): Promise<string> {
  const end = Date.now() + WAIT_MS;
  const hit = (all: string[]) => all.find((l) => (typeof want === 'string' ? l === want : want.test(l)));
  let all = await lines(ctx);
  while (!hit(all) && Date.now() < end) {
    await new Promise((r) => setTimeout(r, 100));
    all = await lines(ctx);
  }
  const found = hit(all);
  if (!found) throw new Error(`${label}: no Settings line ${String(want)} in ${JSON.stringify(all)}`);
  return found;
}

const semanticIndexLine = (ctx: Ctx): Promise<string> => ctx.browser.evaluate((_: null) =>
  (document.querySelector('[role=status].aur-set-index-line') as HTMLElement | null)?.innerText.trim() ?? '', null);

async function consentAndDownload(ctx: Ctx, indexing: string): Promise<void> {
  await ctx.screen.getByRole('checkbox', SWITCH).tap();
  const consent = ctx.screen.getByRole('dialog', 'Turn on semantic search?');
  await expect(consent).toBeVisible();
  const body = await ctx.browser.evaluate((_: null) => Array.from(document.querySelectorAll('[role=dialog], [role=alertdialog]'))
    .map((d) => (d as HTMLElement).innerText).find((t) => t.includes('Turn on semantic search?')) ?? '', null);
  if (!body.includes(indexing)) throw new Error(`consent without "${indexing}": ${body}`);
  await consent.getByRole('button', 'Download and turn on').tap();
}

async function compendiumFooter(ctx: Ctx): Promise<string> {
  await ctx.screen.getByRole('button', 'Close').last().tap();
  await ctx.screen.getByLabel('Compendium search').fill('fireball');
  await pressKey(ctx, 'Enter');
  await expect(ctx.screen.getByRole('dialog', 'Compendium')).toBeVisible();
  return ctx.browser.evaluate((_: null) => (document.querySelector('.v4-comp-foot') as HTMLElement).innerText.trim(), null);
}

describe('s34', { tags: ['s34'] }, () => {
  test('desktop: keyword-only, downloading, indexing, then keyword + semantic', async (ctx) => {
    await bootToHome(ctx, '/?mockSemanticStep=400#/characters');
    const external = await blockExternal(ctx);
    await openSearchSettings(ctx);
    await waitLine(ctx, 'keyword-only', KEYWORD_ONLY);
    expectEqual('no semantic index line while keyword-only', '', await semanticIndexLine(ctx));

    await consentAndDownload(ctx, 'Indexing takes a few minutes.');
    await waitLine(ctx, 'downloading', DOWNLOADING);
    await waitLine(ctx, 'indexing', BUILDING);
    await waitLine(ctx, 'switch while indexing', ON_BUILDING);
    await waitLine(ctx, 'keyword + semantic', READY);
    await waitLine(ctx, 'switch when ready', ON_READY);
    expectEqual('semantic index line', READY, await semanticIndexLine(ctx));

    expectEqual('compendium footer', 'Keyword + semantic', await compendiumFooter(ctx));
    expectEqual('requests that left the device', '', external.join(' '));
  });

  test('phone too slow for the budget: honest consent, then kept keyword-only for speed', async (ctx) => {
    await bootToHome(ctx, '/?mock=mobile&mockSemanticStep=200&mockSemanticQueryMs=95#/characters');
    const external = await blockExternal(ctx);
    await openSearchSettings(ctx);
    await waitLine(ctx, 'keyword-only', KEYWORD_ONLY);

    await consentAndDownload(ctx, PHONE_INDEXING);
    await waitLine(ctx, 'downloading', DOWNLOADING);
    await waitLine(ctx, 'kept keyword-only for speed', TOO_SLOW);
    await waitLine(ctx, 'switch stays on keywords', ON_KEYWORDS);
    expectEqual('semantic index line', TOO_SLOW, await semanticIndexLine(ctx));
    const all = await lines(ctx);
    if (all.includes(READY)) throw new Error('a device over the budget must not claim keyword + semantic');

    expectEqual('compendium footer', 'Keyword search - semantic search unavailable', await compendiumFooter(ctx));
    expectEqual('requests that left the device', '', external.join(' '));
  });
});
