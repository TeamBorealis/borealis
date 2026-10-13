import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { expectEqual, bootToHome, realClick } from '../support/index.js';
import { pressKey, textsOf, waitStatus } from '../support/flows-b.js';

// Application Settings dialog: left vertical tabs, footer Restore defaults / Save / Cancel, Cancel discards, URL unchanged.
const TAB_ORDER = 'GENERAL|APPEARANCE|CHARACTER BUILD|DATA FILES|EQUIPMENT|CHARACTER SHEET|LICENSE|ABOUT';

async function openSettings(ctx: Parameters<typeof bootToHome>[0] & { browser: any }) {
  await ctx.screen.getByRole('menuitem', 'View').tap();
  await ctx.screen.getByRole('menuitem', /^Application settings/i).tap();
  await expect(ctx.screen.getByRole('dialog', 'Application Settings')).toBeVisible();
}
const flowState = (ctx: any): Promise<string> =>
  ctx.browser.evaluate((_: null) => document.querySelector('.aur-status button.flow-on, .aur-status button.flow-off')?.className ?? 'missing', null);

describe('w11', { tags: ['w11'] }, () => {
  test('window geometry, tab order, footer', async (ctx) => {
    const { browser } = ctx;
    await bootToHome(ctx);
    const hashBefore = await browser.evaluate((_: null) => location.hash, null);
    await openSettings(ctx);
    expectEqual('tab order (uppercase)', TAB_ORDER, (await textsOf(ctx, '.aur-set-tabs [role=tab]')).join('|'));
    expectEqual('first tab selected', 'GENERAL', (await textsOf(ctx, '.aur-set-tabs [role=tab][aria-selected=true]')).join('|'));
    expectEqual('footer order', 'RESTORE DEFAULTS|SAVE|CANCEL', (await textsOf(ctx, '.aur-set-foot button')).join('|').toUpperCase());
    expectEqual('modal does not change the URL', hashBefore, await browser.evaluate((_: null) => location.hash, null));
    expectEqual('title (mixed case)', 'Application Settings', (await textsOf(ctx, '.aur-set-head h2'))[0]!);
  });

  test('left tab strip: click and ArrowDown follow the order', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);
    await openSettings(ctx);
    await realClick(ctx, browser.locator('#settings-appearance'), '#settings-appearance');
    expectEqual('appearance selected', 'APPEARANCE', (await textsOf(ctx, '.aur-set-tabs [role=tab][aria-selected=true]')).join(''));
    await pressKey(ctx, 'ArrowDown');
    expectEqual('ArrowDown moves to the next tab', 'CHARACTER BUILD', (await textsOf(ctx, '.aur-set-tabs [role=tab][aria-selected=true]')).join(''));
    await expect(screen.getByText('BUILD')).toBeVisible();
  });

  test('deferred apply: values are held until Save; Cancel and Esc revert; theme previews live', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);
    const nameField = () => browser.evaluate((_: null) => (document.querySelector('.aur-set-win input[type=text], .aur-set-win input:not([type])') as HTMLInputElement).value, null);
    const theme = () => browser.evaluate((_: null) => document.documentElement.getAttribute('data-theme') ?? '(none)', null);
    const themeBefore = await theme();

    // Cancel path: edit the form (and preview a theme swatch), Cancel discards both.
    await openSettings(ctx);
    await screen.getByLabel('PLAYER NAME').fill('Zed Cancelled');
    await realClick(ctx, browser.locator('#settings-appearance'), '#settings-appearance');
    await realClick(ctx, screen.getByRole('radio', /light/i), '.aur-swatches.theme .aur-swatch');
    expectEqual('theme previews live', 'light', await theme());
    await realClick(ctx, screen.getByRole('button', 'Cancel'), '.aur-set-cancel');
    await expect(screen.getByRole('dialog', 'Application Settings')).toBeHidden();
    expectEqual('theme reverted on Cancel', themeBefore, await theme());
    await openSettings(ctx);
    await new Promise((r) => setTimeout(r, 400)); // let the async form load before reading
    expectEqual('player name reverted after Cancel', 'Player One', await nameField());
    await pressKey(ctx, 'Escape');
    await expect(screen.getByRole('dialog', 'Application Settings')).toBeHidden();

    // Save path: persisted, status message, visible after reopening.
    await openSettings(ctx);
    await screen.getByLabel('PLAYER NAME').fill('Zed Saved');
    await realClick(ctx, screen.getByRole('button', 'Save'), '.aur-set-save');
    await expect(screen.getByRole('dialog', 'Application Settings')).toBeHidden();
    expectEqual('save status', 'YOUR PREFERENCES HAVE BEEN SAVED.', (await waitStatus(ctx, (s) => /PREFERENCES/i.test(s))).toUpperCase());
    await openSettings(ctx);
    let got = await nameField();
    for (let i = 0; i < 30 && got !== 'Zed Saved'; i++) { await new Promise((r) => setTimeout(r, 100)); got = await nameField(); } // form loads async
    expectEqual('player name persisted', 'Zed Saved', got);
  });

  // The status-bar toggle is the same preference as this checkbox.
  test('Selection Flow setting drives the status-bar toggle', async (ctx) => {
    const { screen } = ctx;
    await bootToHome(ctx);
    expectEqual('selection flow default is off', 'flow-off', await flowState(ctx));
    await openSettings(ctx);
    await realClick(ctx, screen.getByRole('checkbox', 'Selection Flow'), '.aur-set-win input[type=checkbox]');
    expectEqual('not applied before Save', 'flow-off', await flowState(ctx));
    await realClick(ctx, screen.getByRole('button', 'Save'), '.aur-set-save');
    expectEqual('applied after Save', 'flow-on', await flowState(ctx));
  });
});
