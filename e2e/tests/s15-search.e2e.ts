import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';

// S1.5 (local-search-spec.md section 12): a typo in the title-bar search still finds Fireball first, with no
// network beyond the local app; the flyout offers the All | Rules | Lore scopes and opens a rules section.
describe('s15', { tags: ['s15'] }, () => {
  test('firball finds Fireball first offline; Rules scope opens a section with its attribution', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);
    const external: string[] = [];
    await browser.route(/.*/, async (route) => {
      const url = route.request.url;
      if (/^https?:\/\/(127\.0\.0\.1|localhost)(:\d+)?\//.test(url) || url.startsWith('data:')) await route.continue();
      else { external.push(url); await route.abort(); }
    });

    await screen.getByLabel('Compendium search').fill('firball');
    await pressKey(ctx, 'Enter');
    const dlg = screen.getByRole('dialog', 'Compendium');
    await expect(dlg).toBeVisible();
    await browser.locator('.v4-el .n').first().waitFor({ state: 'visible' });
    const first = await browser.evaluate((_: null) => (document.querySelector('.v4-el .n') as HTMLElement | null)?.innerText ?? '', null);
    expectEqual('first result for "firball"', 'Fireball', first);

    const scopes = await browser.evaluate((_: null) => Array.from(document.querySelectorAll('[aria-label="Search scope"] [role=tab]')).map((t) => (t as HTMLElement).innerText), null);
    expectEqual('scope tabs', 'ALL|RULES|LORE', scopes.join('|').toUpperCase());
    expectEqual('footer', 'KEYWORD SEARCH', (await browser.evaluate((_: null) => (document.querySelector('.v4-comp-foot') as HTMLElement).innerText, null)).toUpperCase());

    await screen.getByRole('tab', 'Rules').click();
    await screen.getByLabel('Search the compendium').fill('dash');
    await browser.locator('[aria-label="Compendium results: Rules"] button').first().waitFor({ state: 'visible' });
    await browser.locator('[aria-label="Compendium results: Rules"] button').first().click();
    await expect(browser.locator('.v4-desc article')).toBeVisible();
    const attribution = await browser.evaluate((_: null) => (document.querySelector('.v4-desc .v4-desc-attr') as HTMLElement | null)?.innerText ?? '', null);
    if (!attribution) throw new Error('rules section without its attribution line');

    expectEqual('requests that left the device', '', external.join(' '));
  });
});
