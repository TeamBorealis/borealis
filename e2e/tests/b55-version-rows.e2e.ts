import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { clickByText, clickCss, count, dblClickCss, rowsText, textOf, waitFor } from '../support/flows-a.js';
import type { Ctx } from '../support/flows-a.js';

// B-55 (specs/version-rows-and-filters.md sec. 6, web e2e): one row per name, the version menu, a version switch on the
// chosen row with its toast, and the Edition filter. Deterministic: mock host, no model.
const DB_ROW = 'li.pick-row[data-row="Dragonborn"]';
const DB_VERSIONS = 'button[aria-label="3 versions of Dragonborn"]';
const MENU = '[role="listbox"][aria-label="Versions of Dragonborn"]';
const option = (label: string) => `${MENU} [role="option"][aria-label="${label}"]`;
const SPECIES = '.pick-stack > section:first-child .pick'; // the Species picker (first on the tab), open or collapsed

async function newCharacter(ctx: Ctx) {
  await bootToHome(ctx);
  await clickCss(ctx, 'button.aur-newchar');
  await expect(ctx.screen.getByRole('dialog', 'New character')).toBeVisible();
  await ctx.browser.locator('#v4-name').fill('Version Test');
  await clickByText(ctx, '.flyout button', 'Create character');
  await waitFor(ctx, (_: null) => location.hash === '#/builder/race', null, 'route /builder/race');
  await waitFor(ctx, (css: string) => !!document.querySelector(css), DB_ROW, 'Dragonborn row');
}

const sourceOf = (ctx: Ctx) => textOf(ctx, `${DB_ROW} .pick-src-name`);

describe('b55 version rows', { tags: ['b55'] }, () => {
  test('2024 species: Dragonborn once, pick the 2014 version, choose, switch back, Edition 2014 filter', async (ctx) => {
    await newCharacter(ctx);
    // One row per name: Dragonborn is listed once, with a version count of 3; names are unique.
    expectEqual('Dragonborn rows', '1', String(await count(ctx, DB_ROW)));
    const names = await rowsText(ctx, 'ul[aria-label="Species options"] .pick-name');
    expect(new Set(names).size).toBe(names.length);
    expectEqual('version count', '3', await textOf(ctx, `${DB_ROW} .pick-ver-count`));
    expectEqual('default version', 'Sample Expansion', await sourceOf(ctx));
    // Source names end at the same x on every row.
    const rights = await ctx.browser.evaluate((_: null) => Array.from(document.querySelectorAll('ul[aria-label="Species options"] .pick-src-name'))
      .map((e) => Math.round(e.getBoundingClientRect().right)), null);
    expect(new Set(rights).size).toBe(1);

    // Open the menu and pick the 2014 version: the row shows it with the edition mark.
    await clickCss(ctx, DB_VERSIONS);
    await waitFor(ctx, (css: string) => !!document.querySelector(css), MENU, 'version menu');
    const heads = await rowsText(ctx, `${MENU} .vm-head`);
    expect(heads).toEqual(['2024 RULES (YOUR CHARACTER)', '2014 RULES']);
    await clickCss(ctx, option('Sample Reference 5.1'));
    await waitFor(ctx, (css: string) => !document.querySelector(css), MENU, 'menu closed');
    expectEqual('shown version', 'Sample Reference 5.1 (2014)', await sourceOf(ctx));

    // Choose it (double-click): the footer names the 2014 source.
    await dblClickCss(ctx, `${DB_ROW} button.pick-main`);
    await waitFor(ctx, (css: string) => (document.querySelector(css)?.textContent ?? '') === 'Sample Reference 5.1 (2014)', `${SPECIES} .pick-chosen-src`, 'chosen footer');

    // Switch back via the menu on the chosen row: switchVersion keeps it chosen and says so.
    await clickCss(ctx, `${SPECIES} .aur-expander-toggle`);
    await waitFor(ctx, (css: string) => !!document.querySelector(css), DB_VERSIONS, 'species list open');
    await clickCss(ctx, DB_VERSIONS);
    await waitFor(ctx, (css: string) => !!document.querySelector(css), MENU, 'version menu');
    expectEqual('chosen marked', 'Sample Reference 5.1, chosen', await ctx.browser.evaluate((css: string) => document.querySelector(css)?.getAttribute('aria-label') ?? '', `${MENU} [role="option"][aria-selected="true"]`));
    await clickCss(ctx, option('Sample Expansion'));
    await waitFor(ctx, (_: null) => Array.from(document.querySelectorAll('.toast')).some((t) => (t.textContent ?? '').includes('Switched Dragonborn to Sample Expansion (2024 rules).')), null, 'switch toast');
    await waitFor(ctx, (css: string) => (document.querySelector(css)?.textContent ?? '') === 'Sample Expansion', `${SPECIES} .pick-chosen-src`, 'footer after switch');

    // Edition 2014: rows with a 2014 (or edition-neutral) version stay, and the chosen row stays with its chosen version.
    await clickCss(ctx, `${SPECIES} .pick-seg-btn:nth-child(3)`);
    await waitFor(ctx, (css: string) => document.querySelector(css)?.getAttribute('aria-pressed') === 'true', `${SPECIES} .pick-seg-btn:nth-child(3)`, 'Edition 2014 on');
    expectEqual('chosen row keeps its version', 'Sample Expansion', await sourceOf(ctx));
    await clickCss(ctx, DB_VERSIONS.replace('3 versions', '2 versions'));
    await waitFor(ctx, (css: string) => !!document.querySelector(css), MENU, 'filtered menu');
    expect(await rowsText(ctx, `${MENU} .vm-src`)).toEqual(['Sample Expansion', 'Sample Reference 5.1']);
  });
});
