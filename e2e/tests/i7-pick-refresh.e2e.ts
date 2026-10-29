import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome } from '../support/index.js';
import { clickByText, clickCss, dblClickCss, rowsText, waitFor } from '../support/flows-a.js';
import type { Ctx } from '../support/flows-a.js';

// GitHub #7: after a pick, the other open lists of the tab show what the character can pick now, without being collapsed
// and reopened. A Rogue has four "Rogue skill" lists; a skill taken in one leaves the others at once, the second list keeps
// its filter text and is never remounted. Deterministic: mock host, no model.
const SKILL_LIST = 'ul[aria-label="Rogue skill options"]';
const nthPick = (n: number) => `.pick-stack > section:nth-of-type(${n}) .pick`;

async function newRogue(ctx: Ctx) {
  await bootToHome(ctx);
  await clickCss(ctx, 'button.aur-newchar');
  await expect(ctx.screen.getByRole('dialog', 'New character')).toBeVisible();
  await ctx.browser.locator('#v4-name').fill('Tree Test');
  await clickByText(ctx, '.flyout button', 'Create character');
  await waitFor(ctx, (_: null) => location.hash === '#/builder/race', null, 'route /builder/race');
  await clickByText(ctx, '[role=tab]', 'Class');
  await waitFor(ctx, (_: null) => !!document.querySelector('li.pick-row[data-row="Rogue"]'), null, 'Rogue row');
  await dblClickCss(ctx, 'li.pick-row[data-row="Rogue"] button.pick-main');
  await waitFor(ctx, (_: null) => (document.querySelector('.pick-chosen-name')?.textContent ?? '') === 'Rogue', null, 'Rogue chosen');
  await clickByText(ctx, '[role=tab]', 'Proficiencies');
  await waitFor(ctx, (css: string) => document.querySelectorAll(`${css} li[data-row="Athletics"]`).length === 4, SKILL_LIST, 'four open skill lists');
}

describe('#7 pick lists refresh', { tags: ['i7'] }, () => {
  test('a skill taken in one open list leaves the other open lists at once, keeping their filter', async (ctx) => {
    await newRogue(ctx);
    const second = nthPick(2);
    // The second list filters on "th" (Athletics, Stealth) and is marked, to show it is never remounted.
    await ctx.browser.locator(`${second} input.pick-fsearch`).fill('th');
    await waitFor(ctx, (css: string) => document.querySelectorAll(`${css} li[data-row]`).length === 2, `${second} ul.pick-list`, 'filtered to two rows');
    await ctx.browser.evaluate((css: string) => { (document.querySelector(css) as HTMLElement).dataset.i7 = 'kept'; return true; }, `${second} ul.pick-list`);

    await dblClickCss(ctx, `${nthPick(1)} li.pick-row[data-row="Athletics"] button.pick-main`);

    await waitFor(ctx, (css: string) => !document.querySelector(`${css} li[data-row="Athletics"]`), `${second} ul.pick-list`, 'Athletics gone from the second list');
    expect(await rowsText(ctx, `${second} li[data-row] .pick-name`)).toEqual(['Stealth']);
    expect(await ctx.browser.evaluate((css: string) => (document.querySelector(css) as HTMLElement | null)?.dataset.i7 ?? '', `${second} ul.pick-list`)).toBe('kept');
    expect(await ctx.browser.evaluate((css: string) => (document.querySelector(css) as HTMLInputElement).value, `${second} input.pick-fsearch`)).toBe('th');
    for (const n of [3, 4]) {
      expect(await ctx.browser.evaluate((css: string) => !!document.querySelector(css), `${nthPick(n)} li[data-row="Athletics"]`)).toBe(false);
      expect(await ctx.browser.evaluate((css: string) => !!document.querySelector(css), `${nthPick(n)} li[data-row="Stealth"]`)).toBe(true);
    }
  });
});
