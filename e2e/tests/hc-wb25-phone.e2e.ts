import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { expectEqual, settledBox } from '../support/index.js';
import { rawTextOf, textOf, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-25, the phone pass (spec 16) on the web mock at phone size (`?creator=1&mock=mobile`, 390x844):
// 1. the editor keeps the element's name and the sub-tab strip on screen while the form scrolls, the pinned Undo / Redo /
//    Try it / Save row sits flush on the status line (no strip of the form between them), and a multi-select picker's
//    row checks and clears with a tap on its name (a check box, not a ring);
// 2. the device-check flow: a pack, a spell with a rule block made through the sheets (U22 select), a weapon, Save,
//    Try it (the full-height sheet), then Export (Share) and Import pack as a copy, with the copy opened in the creator;
// 3. the class LEVELS rows with a sheet per level.
// Deterministic: no model.

const NAME = '#hb-f-name-input';
const PHONE = { width: 390, height: 844 } as const;

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

/** Scrolls an element of the page into the middle of the screen and waits until it holds still (sheets slide in). */
async function reveal(ctx: Ctx, css: string): Promise<void> {
  await ctx.browser.evaluate((sel: string) => {
    const el = document.querySelector(sel);
    el?.scrollIntoView({ block: 'center' });
    return el !== null;
  }, css);
  await settledBox(ctx.browser.locator(css).first());
}

async function openPhone(ctx: Ctx): Promise<void> {
  await ctx.browser.setViewport(PHONE);
  await ctx.app.open('/?creator=1&mock=mobile#/homebrew');
  await expect(ctx.screen.getByRole('button', 'New pack').first()).toBeVisible();
}

async function newPack(ctx: Ctx, name: string): Promise<void> {
  await ctx.screen.getByRole('button', 'New pack').first().tap();
  await ctx.browser.locator('#hb-pack-name').fill(name);
  await ctx.screen.getByRole('button', 'Create').tap();
  await expect(ctx.screen.getByRole('heading', `Homebrew: ${name}`)).toBeAttached();
}

/** The list's New -> a kind's card -> a name -> Create. */
async function newElement(ctx: Ctx, kind: RegExp, name: string): Promise<void> {
  await ctx.screen.getByRole('button', 'New').first().tap();
  await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
  await ctx.browser.locator('#hb-new-name').fill(name);
  await ctx.screen.getByRole('radio', kind).tap();
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, name);
}

/** A select on a phone is the app's bottom-sheet list (U22): open it, tap the option. */
async function sheetSelect(ctx: Ctx, css: string, option: string): Promise<void> {
  await reveal(ctx, css);
  await ctx.browser.locator(css).click();
  await ctx.screen.getByRole('option', option).tap();
  await waitFor(ctx, ({ sel, want }: { sel: string; want: string }) => (document.querySelector(`${sel} .ph-select-value`)?.textContent ?? '') === want,
    { sel: css, want: option }, `${css} shows ${option}`);
}

/** The pinned commands row (spec 16, PH2). */
const pinned = (ctx: Ctx, label: string) => ctx.browser.locator(`.hb-commands.eq-pin button[aria-label="${label}"]`);

/** Keep the next download in the page instead of saving it (the bytes of the blob the link points at). */
async function catchDownloads(ctx: Ctx): Promise<void> {
  await ctx.browser.evaluate((_: null) => {
    const w = window as unknown as { __hbBlob?: Blob; __hbName?: string };
    // the web view of a phone shares the file; this browser has no share sheet, so the page falls back to a download
    Object.defineProperty(navigator, 'share', { value: undefined, configurable: true });
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

async function takeDownload(ctx: Ctx): Promise<{ name: string; base64: string }> {
  await waitFor(ctx, (_: null) => (window as unknown as { __hbName?: string }).__hbName !== undefined, null, 'the pack file download');
  return ctx.browser.evaluate(async (_: null) => {
    const w = window as unknown as { __hbBlob: Blob; __hbName: string };
    const bytes = new Uint8Array(await w.__hbBlob.arrayBuffer());
    let s = '';
    for (const b of bytes) s += String.fromCharCode(b);
    return { name: w.__hbName, base64: btoa(s) };
  }, null);
}

describe('hc-wb25 homebrew creator on a phone (web mock)', { tags: ['homebrew'] }, () => {
  test('the sticky head, the pinned row flush on the status line, and multi-select rows that check with a tap', async (ctx) => {
    await openPhone(ctx);
    await newPack(ctx, 'Ember Isles');
    await newElement(ctx, /^Spell A spell/, 'Ember Lance');

    // Scrolled to the end of the form: the element's name and the sub-tab strip are still on screen, under the app's tabs.
    await ctx.browser.evaluate((_: null) => { const c = document.querySelector('.aur-content')!; c.scrollTop = c.scrollHeight; return true; }, null);
    await waitFor(ctx, (_: null) => {
      const top = document.querySelector('.aur-content')!.getBoundingClientRect().top;
      const head = document.querySelector('.hb-editor-head')!.getBoundingClientRect();
      return Math.abs(head.top - top) < 1 && document.querySelector('.aur-content')!.scrollTop > 200;
    }, null, 'the head stuck at the top of the scrolled page');
    expectEqual('the head names the element', 'Ember Lance', await textOf(ctx, '.hb-editor-head h2'));
    await expect(ctx.screen.getByRole('tab', 'GENERAL')).toBeVisible();

    // The pinned row reaches the page's bottom edge, at the end of the form and in the middle of it: no strip of the form
    // shows between the row and the status line under the page.
    const reaches = (_: null) => {
      const page = document.querySelector('.aur-content')!.getBoundingClientRect();
      return document.querySelector('.hb-commands.eq-pin')!.getBoundingClientRect().bottom >= page.bottom;
    };
    expectEqual('the pinned row on the status line (end of the form)', 'true', String(await ctx.browser.evaluate(reaches, null)));
    await ctx.browser.evaluate((_: null) => { const c = document.querySelector('.aur-content')!; c.scrollTop = c.scrollHeight / 3; return true; }, null);
    expectEqual('the pinned row on the status line (mid form)', 'true', String(await ctx.browser.evaluate(reaches, null)));

    // With the keyboard up (the app sets `kb-open` and `--kb`), the rich-text toolbar is one row docked above it and the
    // pinned commands step aside.
    await ctx.browser.evaluate((_: null) => { document.querySelector('.aur-content')!.scrollTop = 0; return true; }, null);
    await ctx.screen.getByRole('tab', 'DESCRIPTION').tap();
    await ctx.browser.locator('.rich-edit').first().click();
    await ctx.browser.evaluate((_: null) => { document.documentElement.classList.add('kb-open'); document.documentElement.style.setProperty('--kb', '300px'); return true; }, null);
    const docked = await ctx.browser.evaluate((_: null) => {
      const bar = document.querySelector('.rich-docked .rich-toolbar')!.getBoundingClientRect();
      const pin = getComputedStyle(document.querySelector('.hb-commands.eq-pin')!).display;
      return `${bar.height < 80}:${Math.abs(innerHeight - 300 - 8 - bar.bottom) < 2}:${pin}`;
    }, null);
    expectEqual('the toolbar docked above the keyboard, the pinned row hidden', 'true:true:none', docked);
    await ctx.browser.evaluate((_: null) => { document.documentElement.classList.remove('kb-open'); document.documentElement.style.setProperty('--kb', '0px'); return true; }, null);
    await ctx.screen.getByRole('tab', 'GENERAL').tap();

    // A class's Armor training picker keeps several choices: a tap on a row's name checks it, a second tap clears it.
    await ctx.browser.evaluate((_: null) => { document.querySelector('.aur-content')!.scrollTop = 0; return true; }, null);
    await ctx.screen.getByRole('button', /^Back to/).tap();
    await newElement(ctx, /^Class A class/, 'Ember Knight');
    await ctx.screen.getByRole('tab', 'PROFICIENCIES').tap();
    await reveal(ctx, '[data-field="armor"] .hb-pick-btn');
    await ctx.browser.locator('[data-field="armor"] .hb-pick-btn').click();
    await expect(ctx.screen.getByRole('dialog', /^Choose/)).toBeVisible();
    const row = '.hb-pick-list .pick-row:first-child';
    await settledBox(ctx.browser.locator(`${row} button.pick-main`));
    const mark = async () => ctx.browser.evaluate((sel: string) => {
      const m = document.querySelector(`${sel} .pick-mark`);
      return `${m?.classList.contains('is-check')}:${m?.getAttribute('aria-pressed')}`;
    }, row);
    expectEqual('a check box, clear', 'true:false', await mark());
    await ctx.browser.locator(`${row} button.pick-main`).click();
    expectEqual('checked by a tap on the name', 'true:true', await mark());
    await ctx.browser.locator(`${row} button.pick-main`).click();
    expectEqual('cleared by a second tap', 'true:false', await mark());
    await ctx.screen.getByRole('button', 'Cancel').last().tap();
  });

  test('a pack, a spell with a rule from the sheets, a weapon, save, Try it, then export and import it as a copy', async (ctx) => {
    await openPhone(ctx);
    await newPack(ctx, 'Ember Isles');

    // the spell: its school from the U22 sheet, a rule block through the Add menu sheet and the block dialog sheet
    await newElement(ctx, /^Spell A spell/, 'Ember Lance');
    await sheetSelect(ctx, '#hb-f-school-input', 'Evocation');
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await ctx.screen.getByRole('button', 'Add rule').tap();
    await ctx.screen.getByRole('button', 'Advantage on a roll').tap();
    await expect(ctx.screen.getByRole('dialog', 'New rule: Advantage on a roll')).toBeVisible();
    await sheetSelect(ctx, '[data-field="roll"] .ph-select', 'Saving throws');
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', /Advantage on a roll \(Saving throws/)).toBeVisible();
    await autosaved(ctx);
    await pinned(ctx, 'Save').click();
    await waitFor(ctx, (_: null) => /Saved 'Ember Lance' to Ember Isles\./.test(document.querySelector('.aur-status')?.textContent ?? ''), null, 'the spell saved');

    // Try it: the full-height sheet with the test character
    await pinned(ctx, 'Try it').click();
    await expect(ctx.screen.getByRole('dialog', 'Try it')).toBeVisible();
    await waitFor(ctx, (_: null) => /level 1/.test(document.querySelector('.hb-try-status')?.textContent ?? ''), null, 'the test character');
    await ctx.screen.getByRole('button', 'Close').last().tap();
    await expect(ctx.screen.getByRole('dialog', 'Try it')).toBeHidden();

    // the weapon
    await ctx.screen.getByRole('button', /^Back to/).tap();
    await newElement(ctx, /^Weapon A weapon/, 'Sickle Sword');
    await ctx.browser.locator('#hb-f-damage-input').fill('1d6');
    await autosaved(ctx);
    await pinned(ctx, 'Save').click();
    await waitFor(ctx, (_: null) => /Saved 'Sickle Sword' to Ember Isles\./.test(document.querySelector('.aur-status')?.textContent ?? ''), null, 'the weapon saved');

    // Export pack (Share on a phone: the first tap makes the file, the second shares it)
    await ctx.screen.getByRole('button', /^Back to/).tap();
    await ctx.screen.getByRole('button', /^All packs/).tap();
    await expect(ctx.screen.getByRole('grid', 'Homebrew packs')).toBeVisible();
    await catchDownloads(ctx);
    await ctx.browser.locator('.hb-pack-grid .v4-grid-row').first().click();
    await ctx.screen.getByRole('button', 'Export pack').tap();
    await expect(ctx.screen.getByRole('dialog', 'Export pack')).toBeVisible();
    await ctx.browser.locator('[role="dialog"] button:text-is("Share")').click();
    await expect(ctx.screen.getByText(/is ready to share\.$/)).toBeVisible();
    await ctx.browser.locator('[role="dialog"] button:text-is("Share")').click();
    const file = await takeDownload(ctx);
    expectEqual('the pack file name', 'Ember_Isles.borealis-pack.zip', file.name);

    // Import pack: the same file, as a copy; the copy opens in the creator with both elements
    const saved = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'hc-wb25-')), file.name);
    fs.writeFileSync(saved, Buffer.from(file.base64, 'base64'));
    await ctx.browser.locator('input[aria-label="Choose a pack file"]').setInputFiles(saved);
    await expect(ctx.screen.getByRole('dialog', 'Import preview')).toBeVisible();
    await ctx.screen.getByRole('button', 'Import as a copy').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles (copy)')).toBeAttached();
    await expect(ctx.screen.getByRole('button', 'Spells (1)')).toBeVisible();
    await expect(ctx.screen.getByRole('button', 'Weapons (1)')).toBeVisible();
    await ctx.screen.getByRole('button', 'Ember Lance').tap();
    await waitValue(ctx, NAME, 'Ember Lance');
  });

  test('class LEVELS: one row per level, and a tap opens that level in a sheet', async (ctx) => {
    await openPhone(ctx);
    await newPack(ctx, 'Ember Isles');
    await newElement(ctx, /^Class A class/, 'Ember Knight');
    await ctx.screen.getByRole('tab', 'LEVELS').tap();
    await reveal(ctx, '.hb-level-rows li[data-level="4"] .hb-level-row');
    expectEqual('level 4 row', 'Level 4+2 · Ability Score Improvement', await rawTextOf(ctx, '.hb-level-rows li[data-level="4"] .hb-level-row'));
    await ctx.browser.locator('.hb-level-rows li[data-level="4"] .hb-level-row').click();
    await expect(ctx.screen.getByRole('dialog', 'Level 4')).toBeVisible();
    await expect(ctx.screen.getByRole('button', 'Add a feature at level 4')).toBeVisible();
    await ctx.screen.getByRole('button', 'Close').last().tap();
    await expect(ctx.screen.getByRole('dialog', 'Level 4')).toBeHidden();
  });
});
