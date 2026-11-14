import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual, realClick } from '../support/index.js';
import { waitFor, type Ctx } from '../support/flows-a.js';

// H-arI (HC13 3) on the web mock at 1355x847: the white text boxes of the New pack, New element and New character
// flyouts show a subtle 1 px focus ring, their border in Highlight (#293338, Aurora's focused MetroTextBox), with no
// outline; a box without focus keeps its resting border. Deterministic: no model.

const HIGHLIGHT = 'rgb(41, 51, 56)';

/** The focused box's border colour and outline style, and the resting border of another box (when given). */
async function ring(ctx: Ctx, focused: string): Promise<string> {
  await waitFor(ctx, (sel: string) => document.activeElement === document.querySelector(sel), focused, `${focused} has focus`);
  return ctx.browser.evaluate((sel: string) => {
    const s = getComputedStyle(document.querySelector(sel)!);
    return `${s.borderTopColor} ${s.borderTopWidth} outline:${s.outlineStyle}`;
  }, focused);
}

describe('hc-next flyout text box focus ring (web mock)', { tags: ['homebrew'] }, () => {
  test('New pack, New element and New character name boxes show a 1 px Highlight ring when focused', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    expectEqual('New pack name, focused', `${HIGHLIGHT} 1px outline:none`, await ring(ctx, '#hb-pack-name'));
    await ctx.browser.locator('#hb-pack-author').click();
    expectEqual('New pack author, focused', `${HIGHLIGHT} 1px outline:none`, await ring(ctx, '#hb-pack-author'));
    const resting = await ctx.browser.evaluate((_: null) => getComputedStyle(document.querySelector('#hb-pack-name')!).borderTopColor, null);
    if (resting === HIGHLIGHT) throw new Error('a box without focus keeps its resting border');
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();

    await ctx.screen.getByRole('button', 'New').first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
    expectEqual('New element name, focused', `${HIGHLIGHT} 1px outline:none`, await ring(ctx, '#hb-new-name'));
    await ctx.screen.getByRole('button', 'Cancel').tap();

    await bootToHome(ctx);
    await realClick(ctx, ctx.browser.locator('button.aur-newchar'), 'button.aur-newchar');
    await expect(ctx.screen.getByRole('dialog', 'New character')).toBeVisible();
    await ctx.browser.locator('[role="dialog"] .v4-new .aur-textbox.light input').first().click();
    expectEqual('New character name, focused', `${HIGHLIGHT} 1px outline:none`, await ring(ctx, '[role="dialog"] .v4-new .aur-textbox.light input'));
  });
});
