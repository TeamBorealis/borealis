import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { expectEqual, settledBox, bootToHome, realClick } from '../support/index.js';
import { openCharacterTile, pressKey, textsOf } from '../support/flows-b.js';

// F8 / Character Options: checklist, OK registers, Cancel discards. Cards appear in order (AVERAGE HIT POINTS, FEATS, MULTICLASSING).
const optionState = (ctx: any): Promise<string> =>
  ctx.browser.evaluate((_: null) => Array.from(document.querySelectorAll('.v4-opt')).map((o) => {
    const input = o.querySelector('input[type=checkbox]') as HTMLInputElement;
    return `${(o.querySelector('label') as HTMLElement).innerText.trim().toUpperCase()}=${input.checked ? 1 : 0}`;
  }).join('|'), null);

describe('f8', { tags: ['f8'] }, () => {
  test('F8 opens the options flyout; Cancel discards, OK applies', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);

    // F8 needs a loaded character (no flyout without one).
    await pressKey(ctx, 'F8');
    await expect(screen.getByRole('dialog', 'Character options')).toBeHidden();

    await openCharacterTile(ctx, 'Seraphine Vale');
    await pressKey(ctx, 'F8');
    const dlg = screen.getByRole('dialog', 'Character options');
    await expect(dlg).toBeVisible();

    const names = (await textsOf(ctx, '.v4-opt label')).map((t) => t.replace(/\s+/g, ' ').toUpperCase());
    expectEqual('first three cards in order', 'AVERAGE HIT POINTS|FEATS|MULTICLASSING', names.slice(0, 3).join('|'));
    expectEqual('footer button order', 'OK|Cancel', (await textsOf(ctx, '.flyout .v4-foot button')).join('|'));
    const initial = await optionState(ctx);

    // Cancel discards a toggle.
    await realClick(ctx, screen.getByRole('checkbox', 'Feats'), '.v4-opt:nth-child(2) input[type=checkbox]');
    const toggled = await optionState(ctx);
    if (toggled === initial) throw new Error('toggling Feats changed nothing');
    await realClick(ctx, screen.getByRole('button', 'Cancel'), '.flyout .v4-foot button:last-child');
    await expect(dlg).toBeHidden();
    await pressKey(ctx, 'F8');
    await expect(dlg).toBeVisible();
    await settledBox(browser.locator('.flyout')); // wait for the slide-in to settle
    expectEqual('Cancel discarded the toggle', initial, await optionState(ctx));

    // OK applies; reopening shows the new state.
    await realClick(ctx, screen.getByRole('checkbox', 'Feats'), '.v4-opt:nth-child(2) input[type=checkbox]');
    await realClick(ctx, screen.getByRole('button', 'OK'), '.flyout .v4-foot button:first-child');
    await expect(dlg).toBeHidden();
    await pressKey(ctx, 'F8');
    await expect(dlg).toBeVisible();
    await settledBox(browser.locator('.flyout'));
    expectEqual('OK applied the toggle', toggled, await optionState(ctx));
    await pressKey(ctx, 'Escape');
    await expect(dlg).toBeHidden();
  });
});
