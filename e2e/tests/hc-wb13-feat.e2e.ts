import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { dblClickCss, statusMsg, textOf, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-13 (spec 4.2, 13.4, 18.4 flow 2 on the web mock): the feat form with the creator switched on
// (`?creator=1`, desktop 1355x847). Create a 2024 feat: a General category, a prerequisite from the condition builder
// (character level 4 and Strength 13), a rule block; save. The live description shows the prerequisite sentence the
// form writes from the condition, and a second feat's "Gives a feat" picker offers the first with it. Deterministic: no
// model.

const NAME = '#hb-f-name-input';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

async function newFeat(ctx: Ctx, name: string): Promise<void> {
  // the empty editor's New element, or the element list's New once the pack has an element
  await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
  await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
  await ctx.screen.getByRole('radio', /^Feat/).tap();
  await ctx.browser.locator('#hb-new-name').fill(name);
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, name);
}

/** Set row `n` (1-based) of the prerequisite to a test. */
async function conditionRow(ctx: Ctx, n: number, test: string): Promise<void> {
  await ctx.screen.getByRole('button', 'Add condition').tap();
  const select = `[data-field="requirements"] select[aria-label="Prerequisite, part ${n}: test"]`;
  await waitFor(ctx, (sel: string) => document.querySelector(sel) !== null, select, `prerequisite row ${n}`);
  await ctx.browser.locator(select).selectOption(test);
}

describe('hc-wb13 homebrew feat form (web mock)', { tags: ['homebrew'] }, () => {
  test('create a 2024 feat with a prerequisite and a rule block, save, and pick it from a feat picker', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();
    await newFeat(ctx, 'Ember Heart');

    // The feat form: its sections; a new feat can't be taken twice until said so.
    const sections = await ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-editor .hb-section-title')].map((h) => h.textContent ?? ''), null);
    expectEqual('sections', 'Feat,Prerequisite,Taking it again,Details', sections.join(','));
    expectEqual('repeatable starts off', 'false', String(await ctx.browser.evaluate((_: null) => (document.querySelector('[data-field="repeatable"] input') as HTMLInputElement | null)?.checked === true, null)));

    // A General feat; prerequisite: character level 4 and Strength 13 (the rows' starting values).
    await ctx.browser.locator('#hb-f-category-input').fill('General');
    await conditionRow(ctx, 1, 'Character level at least');
    await conditionRow(ctx, 2, 'Ability score at least');
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();

    // RULES: a rule block from the Add menu.
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await ctx.screen.getByRole('button', 'Add rule').tap();
    await expect(ctx.screen.getByRole('dialog', 'Add a rule')).toBeVisible();
    await ctx.screen.getByRole('button', 'Advantage on a roll').tap();
    await expect(ctx.screen.getByRole('dialog', 'New rule: Advantage on a roll')).toBeVisible();
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', /Advantage on a roll/)).toBeVisible();
    await autosaved(ctx);
    // the live description shows the prerequisite written from the condition
    await waitFor(ctx, (_: null) => /Level 4\+, Strength 13\+/.test(document.querySelector('.hb-ws-desc')?.textContent ?? ''), null, 'prerequisite in the description');

    // Save: in the list, counted in CATEGORIES.
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Ember Heart')).toBeVisible();
    expectEqual('status after save', "Saved 'Ember Heart' to Ember Isles", await statusMsg(ctx));
    await waitFor(ctx, (_: null) => /Feats \(1\)/.test(document.querySelector('[aria-label="Element kinds"]')?.textContent ?? ''), null, 'Feats (1)');

    // Pickable: a second feat's "Gives a feat" picker offers it, with its prerequisite.
    await newFeat(ctx, 'Ember Soul');
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await ctx.screen.getByRole('button', 'Add rule').tap();
    await expect(ctx.screen.getByRole('dialog', 'Add a rule')).toBeVisible();
    // the search opens every group that matches
    await ctx.browser.locator('.hb-add-block input[aria-label="Search rules"]').fill('gives a feat');
    await ctx.screen.getByRole('button', 'Gives a feat').tap();
    await expect(ctx.screen.getByRole('dialog', 'New rule: Gives a feat')).toBeVisible();
    await ctx.screen.getByRole('button', 'Pick: Feat').tap();
    await expect(ctx.screen.getByRole('dialog', /^Choose: Feat/)).toBeVisible();
    await ctx.browser.locator('.hb-picker .pick-fsearch').fill('ember heart');
    const row = '.hb-pick-list .pick-row[data-row="Ember Heart"]';
    await waitFor(ctx, ({ sel }: { sel: string }) => {
      const rows = [...document.querySelectorAll('.hb-pick-list .pick-row')];
      return rows.length === 1 && rows[0] === document.querySelector(sel);
    }, { sel: row }, 'only Ember Heart offered');
    expectEqual('the row carries the prerequisite', 'Ember Heart - Requires Level 4+, Strength 13+', await textOf(ctx, `${row} .pick-name`));
    await dblClickCss(ctx, `${row} button.pick-main`);
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', /Gives a feat \(Ember Heart/)).toBeVisible();

    // The saved feat opens again with its category and prerequisite.
    await ctx.screen.getByRole('button', 'Ember Heart').tap();
    await waitValue(ctx, NAME, 'Ember Heart');
    await waitValue(ctx, '#hb-f-category-input', 'General');
    await waitFor(ctx, (_: null) => document.querySelectorAll('[data-field="requirements"] select[aria-label$=": test"]').length === 2, null, 'two prerequisite rows');
  });
});
