import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { statusMsg, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-19 (spec 4.10, 13.4 on the web mock): the small types with the creator switched on (`?creator=1`,
// desktop 1355x847). Create a Language (exotic, with a script) and save it; then a Deity, whose three required fields
// say what they need until they are filled, and save it. Both are counted in CATEGORIES. Deterministic: no model.

const NAME = '#hb-f-name-input';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

const kindCount = (ctx: Ctx, text: RegExp, what: string): Promise<void> =>
  waitFor(ctx, (re: string) => new RegExp(re).test(document.querySelector('[aria-label="Element kinds"]')?.textContent ?? ''), text.source, what);

/** The sentences shown under the field `key`. */
const fieldMessage = (ctx: Ctx, key: string): Promise<string> =>
  ctx.browser.evaluate((k: string) => document.querySelector(`[data-field="${k}"] .hb-field-msg`)?.textContent ?? '', key);

async function newElement(ctx: Ctx, card: RegExp, name: string): Promise<void> {
  // the empty editor's New element, or the element list's New once the pack has an element
  await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
  await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
  await ctx.screen.getByRole('radio', card).tap();
  await ctx.browser.locator('#hb-new-name').fill(name);
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, name);
}

describe('hc-wb19 homebrew small types (web mock)', { tags: ['homebrew'] }, () => {
  test('create a language and a deity, each saved; the deity says what its required fields need', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();

    // A language: exotic, with its script and speakers.
    await newElement(ctx, /^Language A language/, 'Old Tongue');
    await ctx.browser.locator('#hb-f-rarity-input').selectOption('Exotic (2024: rare)');
    await ctx.browser.locator('#hb-f-script-input').fill('Dwarvish');
    await ctx.browser.locator('#hb-f-speakers-input').fill('Ember folk');
    await waitValue(ctx, '#hb-f-script-input', 'Dwarvish');
    await autosaved(ctx);
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Old Tongue')).toBeVisible();
    expectEqual('status after the language', "Saved 'Old Tongue' to Ember Isles", await statusMsg(ctx));
    await kindCount(ctx, /Languages \(1\)/, 'Languages (1)');

    // A deity: its alignment, domains and symbol each say what they need.
    await newElement(ctx, /^Deity A god/, 'Asha');
    await expect(ctx.screen.getByRole('button', /Problems \(3\)/)).toBeVisible();
    expectEqual('alignment sentence', 'A deity needs an alignment.', await fieldMessage(ctx, 'alignment'));
    expectEqual('domains sentence', 'A deity needs domains.', await fieldMessage(ctx, 'domains'));
    expectEqual('symbol sentence', 'A deity needs a symbol.', await fieldMessage(ctx, 'symbol'));
    await ctx.browser.locator('#hb-f-alignment-input').fill('Chaotic Good');
    await ctx.browser.locator('[data-field="domains"] input').fill('Light');
    await ctx.screen.getByRole('button', 'Add to Domains').tap();
    await expect(ctx.screen.getByRole('button', 'Remove Light')).toBeVisible();
    await ctx.browser.locator('#hb-f-symbol-input').fill('A flame in a hand');
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await autosaved(ctx);
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Asha')).toBeVisible();
    expectEqual('status after the deity', "Saved 'Asha' to Ember Isles", await statusMsg(ctx));
    await kindCount(ctx, /Deities \(1\)/, 'Deities (1)');
    await kindCount(ctx, /Languages \(1\)/, 'Languages (1) still');
  });
});
