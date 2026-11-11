import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-24 (spec 14.2, 18.4 flow 9 on the web mock, desktop 1355x847, `?creator=1`): export a pack as a
// `.borealis-pack.zip` (the web view downloads the bytes the backend answers), then import that file back through the
// pack list's Import pack: the pack is installed already, so the preview offers Replace or Import as a copy; the copy
// opens in the creator with its element, and the original stays. Deterministic: no model.

const NAME = '#hb-f-name-input';

interface Download { name: string; base64: string }

/** Keep the next download in the page instead of saving it (the bytes of the blob the link points at). */
async function catchDownloads(ctx: Ctx): Promise<void> {
  await ctx.browser.evaluate((_: null) => {
    const w = window as unknown as { __hbBlob?: Blob; __hbName?: string };
    const create = URL.createObjectURL.bind(URL);
    URL.createObjectURL = (b: Blob | MediaSource) => { if (b instanceof Blob) w.__hbBlob = b; return create(b); };
    const click = HTMLAnchorElement.prototype.click;
    HTMLAnchorElement.prototype.click = function (this: HTMLAnchorElement) {
      if (this.download) { w.__hbName = this.download; return; }
      click.call(this);
    };
    return true;
  }, null);
}

async function takeDownload(ctx: Ctx): Promise<Download> {
  await waitFor(ctx, (_: null) => (window as unknown as { __hbName?: string }).__hbName !== undefined, null, 'the pack file download');
  return ctx.browser.evaluate(async (_: null) => {
    const w = window as unknown as { __hbBlob: Blob; __hbName: string };
    const bytes = new Uint8Array(await w.__hbBlob.arrayBuffer());
    let s = '';
    for (const b of bytes) s += String.fromCharCode(b);
    return { name: w.__hbName, base64: btoa(s) };
  }, null);
}

const packRows = (ctx: Ctx): Promise<string[]> =>
  ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-pack-grid .v4-grid-row .nm')].map((n) => n.firstChild?.textContent ?? ''), null);

describe('hc-wb24 homebrew pack export and import (web mock)', { tags: ['homebrew'] }, () => {
  test('export a pack, import the file as a copy, and edit the copy', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();

    // one spell, saved
    await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
    await ctx.browser.locator('#hb-new-name').fill('Ember Lance');
    await ctx.screen.getByRole('button', 'Create').tap();
    await waitFor(ctx, (sel: string) => (document.querySelector(sel) as HTMLInputElement | null)?.value === 'Ember Lance', NAME, 'the new spell');
    await ctx.browser.locator('#hb-f-school-input').selectOption('Evocation');
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Ember Lance')).toBeVisible();

    // the pack list: Export pack
    await ctx.browser.evaluate((_: null) => { window.location.hash = '#/homebrew'; return true; }, null);
    await expect(ctx.screen.getByRole('grid', 'Homebrew packs')).toBeVisible();
    await catchDownloads(ctx);
    await ctx.browser.locator('.hb-pack-grid .v4-grid-row').first().click();
    await ctx.screen.getByRole('button', 'Export pack').tap();
    await expect(ctx.screen.getByRole('dialog', 'Export pack')).toBeVisible();
    await ctx.browser.locator('[role="dialog"] button:text-is("Export")').click();
    const file = await takeDownload(ctx);
    expectEqual('the pack file name', 'Ember_Isles.borealis-pack.zip', file.name);

    // Import pack: the same file; the pack is here, so Replace or Import as a copy
    const saved = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'hc-wb24-')), file.name);
    fs.writeFileSync(saved, Buffer.from(file.base64, 'base64'));
    await ctx.browser.locator('input[aria-label="Choose a pack file"]').setInputFiles(saved);
    await expect(ctx.screen.getByRole('dialog', 'Import preview')).toBeVisible();
    await expect(ctx.screen.getByRole('button', 'Replace')).toBeVisible();
    const alert = await ctx.browser.evaluate((_: null) => document.querySelector('[role="dialog"] .alert, [role="dialog"] [role="alert"]')?.textContent ?? '', null);
    expectEqual('the preview says what it adds', '1 element will be added as the homebrew pack "Ember Isles", editable in Homebrew.', alert.trim());
    await ctx.screen.getByRole('button', 'Import as a copy').tap();

    // the copy opens in the creator, editable, with its element
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles (copy)')).toBeAttached();
    await ctx.screen.getByRole('button', 'Ember Lance').tap();
    await waitFor(ctx, (sel: string) => (document.querySelector(sel) as HTMLInputElement | null)?.value === 'Ember Lance', NAME, 'the copied spell opens');
    await ctx.browser.locator(NAME).fill('Ember Lance II');
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Ember Lance II')).toBeVisible();

    // both packs are listed; the original is untouched
    await ctx.browser.evaluate((_: null) => { window.location.hash = '#/homebrew'; return true; }, null);
    await waitFor(ctx, (_: null) => document.querySelectorAll('.hb-pack-grid .v4-grid-row').length === 2, null, 'two packs');
    expectEqual('packs', 'Ember Isles|Ember Isles (copy)', (await packRows(ctx)).join('|'));
  });
});
