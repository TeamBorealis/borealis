import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { expectEqual, settledBox } from '../support/index.js';
import { textOf, waitFor, type Ctx } from '../support/flows-a.js';

// H0eao (spec 12.4, 16) on the web mock at phone size (`?creator=1&mock=mobile`, 390x844): SPELL LISTS' Select mode
// chooses rows with a tap (a phone has no Ctrl/Shift+click), Add selected checks them all at once, and leaving the mode
// clears the choice. A real device check is lane F's. Deterministic: no model.

const PHONE = { width: 390, height: 844 } as const;
const VIEW = '.hb-lists';

/** Taps a spell's name in the grid, as a finger does (the row's mousedown, then the name's click). */
async function tapSpell(ctx: Ctx, name: string): Promise<void> {
  await ctx.browser.evaluate(({ v, n }: { v: string; n: string }) => {
    const b = [...document.querySelectorAll<HTMLButtonElement>(`${v} table button.ap-name`)].find((x) => x.textContent === n);
    b?.scrollIntoView({ block: 'center' });
    return b !== undefined;
  }, { v: VIEW, n: name });
  const button = ctx.browser.locator(`${VIEW} table tr:has(button.ap-name:text-is("${name}")) button.ap-name`).first();
  await settledBox(button);
  await button.tap();
}

const selectedNames = (ctx: Ctx): Promise<string> =>
  ctx.browser.evaluate((v: string) => [...document.querySelectorAll(`${v} table tr[aria-selected="true"] button.ap-name`)].map((b) => b.textContent ?? '').sort().join(','), VIEW);

describe('hc-next SPELL LISTS on a phone (web mock)', { tags: ['homebrew'] }, () => {
  test('Select mode chooses rows with a tap and Add selected checks them', async (ctx) => {
    await ctx.browser.setViewport(PHONE);
    await ctx.app.open('/?creator=1&mock=mobile#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();

    await ctx.screen.getByRole('button', 'Spell Lists').tap();
    await expect(ctx.screen.getByRole('region', 'Spell lists')).toBeVisible();
    await ctx.browser.locator(`${VIEW} .ph-select[aria-label="Spell list"]`).tap();
    await ctx.screen.getByRole('option', 'Wizard').tap();
    await waitFor(ctx, (v: string) => document.querySelector(`${v} table`) !== null, VIEW, 'the spell grid');

    await ctx.screen.getByRole('button', 'Select').tap();
    expectEqual('Select mode is on', 'true', await ctx.browser.evaluate((v: string) =>
      document.querySelector(`${v} [aria-label="Spell list commands"] button[aria-pressed]`)?.getAttribute('aria-pressed') ?? '', VIEW));
    for (const name of ['Bless', 'Guidance', 'Cure Wounds', 'Cure Wounds']) await tapSpell(ctx, name);
    expectEqual('the tapped rows are chosen (a second tap takes one out)', 'Bless,Guidance', await selectedNames(ctx));
    expectEqual('the count of chosen rows', '2 spells chosen', await textOf(ctx, `${VIEW} .hb-lists-chosen`));

    await ctx.screen.getByRole('button', 'Add selected').tap();
    await waitFor(ctx, (v: string) => /\(2 not saved\)/.test(document.querySelector(`${v} .hb-lists-count`)?.textContent ?? ''), VIEW, 'two checks waiting for Save');
    const checks = await ctx.browser.evaluate((v: string) => ['Bless', 'Guidance'].map((n) =>
      (document.querySelector<HTMLInputElement>(`${v} input[aria-label="Choose ${n}"]`)?.checked ?? false) ? 'on' : 'off').join(','), VIEW);
    expectEqual('both spells checked', 'on,on', checks);

    await ctx.screen.getByRole('button', 'Select').tap();
    expectEqual('leaving the mode clears the choice', '', await selectedNames(ctx));
  });
});
