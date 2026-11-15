import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { clickCss, statusMsg, textOf, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-11 (spec 4.1, 13.4, 13.7, 18.4 flow 1 on the web mock): the spell form with the creator switched on
// (`?creator=1`, desktop 1355x847). Create a spell, choose its school and level, put it on the Wizard spell list, save;
// then a second element's "Gives a spell" picker offers it in the Magic grid at its level, with its school and
// components. Deterministic: no model.

const NAME = '#hb-f-name-input';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

async function newElement(ctx: Ctx, name: string): Promise<void> {
  // the empty editor's New element, or the element list's New once the pack has an element
  await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
  await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
  await ctx.browser.locator('#hb-new-name').fill(name);
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, name);
}

describe('hc-wb11 homebrew spell form (web mock)', { tags: ['homebrew'] }, () => {
  test('create a spell, put it on the Wizard list, save, and pick it from a spell picker', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();
    await newElement(ctx, 'Ember Lance');

    // The spell form: its sections, and a new spell's starting values; the school is the one thing left to choose.
    const sections = await ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-editor .hb-section-title')].map((h) => h.textContent ?? ''), null);
    expectEqual('sections', 'Casting,Components,Spell lists,Details', sections.join(','));
    expectEqual('casting time starts as an action', 'Action', await ctx.browser.evaluate((_: null) => (document.querySelector('[data-field="time"] input') as HTMLInputElement | null)?.value ?? '', null));
    await expect(ctx.screen.getByRole('button', /Problems \(1\)/)).toBeVisible();
    await expect(ctx.screen.getByRole('button', 'A spell needs a school.')).toBeVisible();

    // Fill in: 2nd level, Evocation, on the Wizard list.
    await ctx.browser.locator('#hb-f-level-input').selectOption('2nd level');
    await ctx.browser.locator('#hb-f-school-input').selectOption('Evocation');
    await ctx.browser.locator('[data-field="spellLists"] input[aria-label="Wizard"]').check();
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await autosaved(ctx);

    // Save: in the list, counted in CATEGORIES.
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Ember Lance')).toBeVisible();
    expectEqual('status after save', "Saved 'Ember Lance' to Ember Isles", await statusMsg(ctx));
    await waitFor(ctx, (_: null) => /Spells \(1\)/.test(document.querySelector('[aria-label="Element kinds"]')?.textContent ?? ''), null, 'Spells (1)');

    // Pickable: a second spell's "Gives a spell" picker offers it at level 2 with its school and components.
    await newElement(ctx, 'Ember Ward');
    await ctx.browser.locator('#hb-f-school-input').selectOption('Abjuration');
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await ctx.screen.getByRole('button', 'Add rule').tap();
    await ctx.screen.getByRole('button', 'Gives a spell').tap();
    await ctx.screen.getByRole('button', 'Pick: Spell').tap();
    await expect(ctx.screen.getByRole('table', 'Spells')).toBeVisible();
    await clickCss(ctx, '.hb-spell-picker .sp-levels button:nth-child(3)');
    await waitFor(ctx, (_: null) => document.querySelector('.hb-spell-picker input[aria-label="Choose Ember Lance"]') !== null, null, 'Ember Lance offered at level 2');
    const row = '.hb-spell-picker tr:has(input[aria-label="Choose Ember Lance"])';
    expectEqual('level and school', 'Level 2 · Evocation', await textOf(ctx, `${row} .sp-lvl`));
    expectEqual('source', 'Ember Isles', await textOf(ctx, `${row} .sp-src`));
    await ctx.browser.locator('.hb-spell-picker input[aria-label="Choose Ember Lance"]').check();
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', /Gives a spell \(Ember Lance/)).toBeVisible();

    // The saved spell opens again on its list.
    await ctx.screen.getByRole('button', 'Ember Lance').tap();
    await waitValue(ctx, NAME, 'Ember Lance');
    await waitFor(ctx, (_: null) => (document.querySelector('[data-field="spellLists"] input[aria-label="Wizard"]') as HTMLInputElement | null)?.checked === true, null, 'Wizard list checked');
  });
});
