import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { expectEqual, settledBox, bootToHome, realClick } from '../support/index.js';
import { hash, openCharacterTile, pressKey, statusText, textsOf, waitStatus } from '../support/flows-b.js';

// Save and load: Ctrl+S opens the Save flyout, status "'NAME' SAVED", no toast.
describe('w6', { tags: ['w6'] }, () => {
  test('load by double-click, save via Ctrl+S flyout, round trip', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);
    await expect(screen.getByRole('group', 'Seraphine Vale').first()).toBeVisible();

    // Load: double-click the tile; the app jumps to Build > Race.
    await openCharacterTile(ctx, 'Seraphine Vale');
    await expect(screen.getByRole('menuitem', 'File')).toBeVisible();
    expectEqual('after load route', '#/builder/race', await waitHash(ctx, '#/builder/race'));
    expectEqual('load status text', "LOADED 'SERAPHINE VALE'", await waitStatus(ctx, (s) => /LOADED/i.test(s)));

    // Ctrl+S opens the Save flyout, not a toast.
    await pressKey(ctx, 'Control+s');
    await expect(screen.getByRole('dialog', 'Save character')).toBeVisible();
    await settledBox(browser.locator('.flyout')); // settle the slide-in before pointer input
    expectEqual('save flyout footer buttons', 'Save character|Cancel', (await textsOf(ctx, '.flyout .v4-foot button')).join('|'));
    expectEqual('save flyout field labels', 'Filename|Group', (await textsOf(ctx, '.flyout .sv .field label')).join('|'));

    // Change something: favorite + group, then confirm with a real pointer.
    await screen.getByLabel('Group').fill('Party');
    await browser.locator('.sv-fav input[type=checkbox]').tap();
    await realClick(ctx, screen.getByRole('button', 'Save character'), '.flyout .v4-foot button.highlight, .flyout .v4-foot button:first-child');
    await expect(screen.getByRole('dialog', 'Save character')).toBeHidden();
    expectEqual('saved status', "'SERAPHINE VALE' SAVED", await waitStatus(ctx, (s) => /SAVED/i.test(s)));
    expectEqual('no toast after save', 0, await browser.evaluate((_: null) => document.querySelectorAll('[role=alert], .toast').length, null));

    // Round trip: back to the list, the change is in MockApi.
    await browser.evaluate((_: null) => { location.hash = '#/characters'; return 1; }, null);
    await expect(screen.getByRole('group', 'Brannoc Ironvale').first()).toBeVisible();
    const headers = await textsOf(ctx, '.v4-tiles h2, .v4-tiles h3, .v4-group-head, [class*=group] h2, [class*=group] h3');
    if (!headers.some((h) => /party/i.test(h))) throw new Error(`group "Party" not shown after save; headers: ${JSON.stringify(headers)}; status ${await statusText(ctx)}; hash ${await hash(ctx)}`);
  });
});

async function waitHash(ctx: Parameters<typeof hash>[0], want: string): Promise<string> {
  const end = Date.now() + 5000;
  let h = await hash(ctx);
  while (h !== want && Date.now() < end) { await new Promise((r) => setTimeout(r, 100)); h = await hash(ctx); }
  return h;
}
