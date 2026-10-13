import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { expectEqual, settledBox, bootToHome, realClick } from '../support/index.js';
import { openCharacterTile, pressKey, textsOf } from '../support/flows-b.js';

// Portraits: gallery flyout, Ctrl+P opens, Esc closes.
describe('w10', { tags: ['w10'] }, () => {
  test('portrait button and Ctrl+P open the gallery; Browse adds, Select applies', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);
    await openCharacterTile(ctx, 'Seraphine Vale');


    // Ctrl+P opens, Esc closes.
    await pressKey(ctx, 'Control+p');
    const dlg = screen.getByRole('dialog', 'Portraits gallery');
    await expect(dlg).toBeVisible();
    await pressKey(ctx, 'Escape');
    await expect(dlg).toBeHidden();

    // Real-pointer click on the header portrait button.
    await realClick(ctx, browser.locator('.aur-portrait-btn'), '.aur-portrait-btn');
    await expect(dlg).toBeVisible();

    expectEqual('gallery filter labels', 'Show all|Male|Female', (await textsOf(ctx, '.pg-filters label')).join('|'));
    expectEqual('gallery footer button order', 'Select portrait|Browse|Cancel', (await textsOf(ctx, '.pg-buttons button')).join('|'));

    // Browse a PNG (hidden file input), it appears as a tile and Select portrait applies it.
    await browser.locator('input[type=file][aria-label="Add a portrait image"]').setInputFiles('fixtures/portrait.png');
    await browser.locator('.pg-tile[aria-label="Portrait 1"]').waitFor({ state: 'visible' });
    await realClick(ctx, screen.getByRole('button', 'Select portrait'), '.pg-buttons button');
    await expect(dlg).toBeHidden();
    const src = await browser.evaluate((_: null) => (document.querySelector('.aur-portrait-btn img') as HTMLImageElement | null)?.src.slice(0, 21) ?? 'none', null);
    expectEqual('applied portrait is a data URL', 'data:image/png;base64', src);
  });

  test('Cancel leaves the portrait unchanged', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);
    await openCharacterTile(ctx, 'Brannoc Ironvale');
    const before = await browser.evaluate((_: null) => (document.querySelector('.aur-portrait-btn img') as HTMLImageElement | null)?.src ?? 'none', null);
    await realClick(ctx, browser.locator('.aur-portrait-btn'), '.aur-portrait-btn');
    await settledBox(browser.locator('.flyout'));
    await browser.locator('input[type=file][aria-label="Add a portrait image"]').setInputFiles('fixtures/portrait.png');
    await realClick(ctx, screen.getByRole('button', 'Cancel'), '.pg-buttons button:last-child');
    await expect(screen.getByRole('dialog', 'Portraits gallery')).toBeHidden();
    const after = await browser.evaluate((_: null) => (document.querySelector('.aur-portrait-btn img') as HTMLImageElement | null)?.src ?? 'none', null);
    expectEqual('portrait unchanged after Cancel', before, after);
  });
});
