import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { expectEqual, bootToHome } from '../support/index.js';
import { pressKey, realClickText, textsOf } from '../support/flows-b.js';

// Sources list: range selection and the source context menu.
const ROW = '[aria-label="Sources"] [role=row][aria-selected]';
const MENU_ITEM = '.v4-ctx [role=menuitem]';
const rowState = (ctx: any): Promise<string> =>
  ctx.browser.evaluate((css: string) => Array.from(document.querySelectorAll(css)).map((r) =>
    `${r.getAttribute('aria-selected') === 'true' ? 'S' : '-'}${(r.querySelector('input[type=checkbox]') as HTMLInputElement).checked ? 'A' : 'x'}`).join(','), ROW);

describe('sources', { tags: ['sources'] }, () => {
  test('Shift range select + context menu: a real-pointer click on "Toggle all selected sources" flips every selected row', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);
    await browser.evaluate((_: null) => { location.hash = '#/content/sources'; return 1; }, null);
    await expect(screen.getByRole('grid', 'Sources')).toBeVisible();
    const row = (i: number) => (browser.locator(ROW) as any).nth(i);
    const total = await browser.evaluate((css: string) => document.querySelectorAll(css).length, ROW);
    if (total < 2) throw new Error(`need >= 2 source rows, found ${total}`);
    const last = Math.min(total - 1, 2); // seed has 2 sources; with more, "two rows down"

    // Click row 0, Shift+click the last row (<= two rows down): ListView Extended range.
    await row(0).tap();
    await row(last).tap({ modifiers: ['Shift'] });
    const before = await rowState(ctx);
    const selectedIdx = before.split(',').map((s, i) => (s[0] === 'S' ? i : -1)).filter((i) => i >= 0);
    expectEqual('Shift+click selects the range', Array.from({ length: last + 1 }, (_, i) => i).join(','), selectedIdx.join(','));

    // Right-click inside the selection keeps it and opens the menu.
    await row(last).secondaryTap();
    await expect(screen.getByRole('menu', 'Source')).toBeVisible();
    expectEqual('right-click inside the selection keeps it', before.replace(/[Ax]/g, ''), (await rowState(ctx)).replace(/[Ax]/g, ''));
    expectEqual('menu item order', 'ALLOW THIS SOURCE|TOGGLE ALL SELECTED SOURCES|TOGGLE ALL SOURCES BY THE SAME AUTHOR|VIEW IN COMPENDIUM',
      (await textsOf(ctx, MENU_ITEM)).join('|').toUpperCase().replace('RESTRICT THIS SOURCE', 'ALLOW THIS SOURCE'));

    // The item is the topmost element at its centre, and a real pointer click on it flips every selected row.
    const item = screen.getByRole('menuitem', 'Toggle all selected sources');
    await realClickText(ctx, item, MENU_ITEM, 'Toggle all selected sources');
    await expect(screen.getByRole('menu', 'Source')).toBeHidden();
    const after = await rowState(ctx);
    const flipped = before.split(',').map((s, i) => (selectedIdx.includes(i) ? (s[1] === 'A' ? 'x' : 'A') : s[1])).join(',');
    expectEqual('each selected row allow flag flipped', flipped, after.split(',').map((s) => s[1]).join(','));
  });

  test('Escape closes the menu without changing any row', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);
    await browser.evaluate((_: null) => { location.hash = '#/content/sources'; return 1; }, null);
    await expect(screen.getByRole('grid', 'Sources')).toBeVisible();
    const before = await rowState(ctx);
    await (browser.locator(ROW) as any).nth(0).secondaryTap();
    await expect(screen.getByRole('menu', 'Source')).toBeVisible();
    await pressKey(ctx, 'Escape');
    await expect(screen.getByRole('menu', 'Source')).toBeHidden();
    expectEqual('rows unchanged after Escape', before.split(',').map((s) => s[1]).join(','), (await rowState(ctx)).split(',').map((s) => s[1]).join(','));
  });
});
