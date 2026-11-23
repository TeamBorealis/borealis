import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, realClick } from '../support/index.js';
import { clickByText, clickCss, waitFor } from '../support/flows-a.js';
import type { Ctx } from '../support/flows-a.js';

// Linked Elements (opt-in, DISPLAY tab): with the setting on, a highlighted option shows the cards of the elements it
// grants or names below its own description; with it off the pane has no such section. Mock host, no model.
const LINKED = 'section[aria-label="Linked elements"]';
const linkedText = (ctx: Ctx): Promise<string> =>
  ctx.browser.evaluate((css: string) => document.querySelector(css)?.textContent ?? '', LINKED);

async function highlightCleric(ctx: Ctx) {
  await clickCss(ctx, 'button.aur-newchar');
  await expect(ctx.screen.getByRole('dialog', 'New character')).toBeVisible();
  await ctx.browser.locator('#v4-name').fill('Link Test');
  await clickByText(ctx, '.flyout button', 'Create character');
  await waitFor(ctx, (_: null) => location.hash === '#/builder/race', null, 'route /builder/race');
  await clickByText(ctx, '[role=tab]', 'Class');
  await waitFor(ctx, (_: null) => !!document.querySelector('li.pick-row[data-row="Cleric"]'), null, 'Cleric row');
  await clickCss(ctx, 'li.pick-row[data-row="Cleric"] button.pick-main');
  await waitFor(ctx, (_: null) => /Cleric/.test(document.querySelector('.desc-pane')?.textContent ?? ''), null, 'Cleric described');
}

describe('linked elements', { tags: ['linked-elements'] }, () => {
  test('off by default; on from DISPLAY, a highlighted option shows its linked card', async (ctx) => {
    const { screen } = ctx;
    await bootToHome(ctx);
    await highlightCleric(ctx);
    expect(await linkedText(ctx)).toBe('');

    await screen.getByRole('menuitem', 'View').tap();
    await screen.getByRole('menuitem', /^Application settings/i).tap();
    await expect(screen.getByRole('dialog', 'Application Settings')).toBeVisible();
    await realClick(ctx, ctx.browser.locator('#settings-display'), '#settings-display');
    await realClick(ctx, screen.getByRole('checkbox', 'Linked Elements'), '.aur-set-win input[type=checkbox]');
    await realClick(ctx, screen.getByRole('button', 'Save'), '.aur-set-save');
    await expect(screen.getByRole('dialog', 'Application Settings')).toBeHidden();

    await waitFor(ctx, (css: string) => /Cure Wounds/.test(document.querySelector(css)?.textContent ?? ''), LINKED, 'the Cure Wounds card');
    expect(await linkedText(ctx)).toContain('Restore hit points by touch.');
  });
});
