import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { expectEqual, bootToHome, realClick } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';

// Compendium: bottom flyout, search placeholder, F1 toggle.
describe('w9', { tags: ['w9'] }, () => {
  test('Alt+Q search, Enter opens bottom flyout, select shows description, F1 toggles', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);

    expectEqual('search placeholder', 'Compendium Search (Alt+Q)',
      await browser.evaluate((_: null) => (document.querySelector('input[aria-label="Compendium search"]') as HTMLInputElement).placeholder, null));

    // Alt+Q focuses the quick search box.
    await pressKey(ctx, 'Alt+q');
    expectEqual('Alt+Q focus target', 'Compendium search',
      await browser.evaluate((_: null) => document.activeElement?.getAttribute('aria-label') ?? '', null));

    await screen.getByLabel('Compendium search').fill('fireball');
    await pressKey(ctx, 'Enter');
    const dlg = screen.getByRole('dialog', 'Compendium');
    await expect(dlg).toBeVisible();


    // Results update live: the query reached the browser, Fireball is listed.
    const row = browser.locator('.v4-el button[aria-selected]');
    await row.first().waitFor({ state: 'visible' });
    const names = await browser.evaluate((_: null) => Array.from(document.querySelectorAll('.v4-el .n')).map((e) => (e as HTMLElement).innerText), null);
    if (!names.includes('Fireball')) throw new Error(`Fireball not in results: ${JSON.stringify(names)}`);

    // Select a row: description on the right.
    await expect(screen.getByText('Select an entry to read it.')).toBeVisible();
    await realClick(ctx, browser.locator('.v4-el button[aria-selected]').first(), '.v4-el button[aria-selected]');
    await expect(browser.locator('.v4-desc article')).toBeVisible();
    expectEqual('selected row marks aria-selected', 'true', await browser.evaluate((_: null) => document.querySelector('.v4-el button[aria-selected="true"]') ? 'true' : 'false', null));

    // Narrowing by filter: type then source filters exist (labels).
    await expect(screen.getByLabel('Filter by type')).toBeVisible();
    await expect(screen.getByLabel('Filter by source')).toBeVisible();

    // F1 / Esc close; F1 toggles open again.
    await pressKey(ctx, 'Escape');
    await expect(dlg, 'Esc closes').toBeHidden();
    await pressKey(ctx, 'F1');
    await expect(dlg).toBeVisible();
  });

  // Close with the same shortcut (F1 toggles the compendium flyout).
  test('F1 toggles the flyout closed', async (ctx) => {
    const { screen } = ctx;
    await bootToHome(ctx);
    await pressKey(ctx, 'F1');
    await expect(screen.getByRole('dialog', 'Compendium')).toBeVisible();
    await pressKey(ctx, 'F1');
    await expect(screen.getByRole('dialog', 'Compendium')).toBeHidden();
  });
});
