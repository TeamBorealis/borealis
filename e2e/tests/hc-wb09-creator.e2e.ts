import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { clickCss, dblClickCss, statusMsg, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-09 (spec 2, 10.2, 12.2, 13.1-13.3): START > HOMEBREW on the web mock with the creator switched on
// (`?creator=1`). Open a pack -> its workspace -> edit an element -> undo. And the switch: without `?creator=1` (the 1.4.x
// default) the HOMEBREW sub-tab is not there and its route goes back to the characters. Deterministic: no model.

const NAME = '#hb-f-name-input';

const inputValue = (ctx: Ctx, css: string): Promise<string> =>
  ctx.browser.evaluate((sel: string) => (document.querySelector(sel) as HTMLInputElement | null)?.value ?? '', css);

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

/** START > HOMEBREW > NEW PACK "Ember Isles", then back to the pack list. */
async function makePack(ctx: Ctx): Promise<void> {
  await ctx.screen.getByRole('link', 'Homebrew').tap();
  await expect(ctx.screen.getByRole('grid', 'Homebrew packs')).toBeVisible();
  await ctx.screen.getByRole('button', 'New pack').tap();
  await expect(ctx.screen.getByRole('dialog', 'New pack')).toBeVisible();
  await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
  expectEqual('code suggested from the name', 'EI', await inputValue(ctx, '#hb-pack-code'));
  await ctx.screen.getByRole('button', 'Create').tap();
  await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();
  await ctx.screen.getByRole('button', 'All packs').tap();
  await expect(ctx.screen.getByRole('grid', 'Homebrew packs')).toBeVisible();
}

describe('hc-wb09 homebrew creator shell (web mock)', { tags: ['homebrew'] }, () => {
  test('open pack -> workspace -> edit -> undo (button and Ctrl+Z), redo, save', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/characters');
    await makePack(ctx);

    // Open the pack from the grid (double-click, as the Sources grid opens).
    await dblClickCss(ctx, '[aria-label="Homebrew packs"] [role=row][aria-selected]');
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();
    await expect(ctx.screen.getByRole('navigation', 'Element kinds')).toBeVisible();

    // New element: a spell named Ember Bolt.
    await ctx.screen.getByRole('button', 'New element').first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
    await ctx.browser.locator('#hb-new-name').fill('Ember Bolt');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('region', 'Element editor')).toBeVisible();
    await waitValue(ctx, NAME, 'Ember Bolt');

    // Edit: the name, then a combo (a separate undo step).
    await ctx.browser.locator(NAME).fill('Ember Lance');
    await ctx.browser.locator('#hb-f-school-input').selectOption('Evocation');
    await waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

    // Undo by the button (the school), then by Ctrl+Z (the name).
    await clickCss(ctx, '.hb-commands button[aria-label="Undo"]');
    expectEqual('school undone', '', await inputValue(ctx, '#hb-f-school-input'));
    expectEqual('name kept after one undo', 'Ember Lance', await inputValue(ctx, NAME));
    await pressKey(ctx, 'Control+z');
    await waitValue(ctx, NAME, 'Ember Bolt');

    // Redo by Ctrl+Shift+Z, then Ctrl+S saves the element (on this route it never saves a character).
    await pressKey(ctx, 'Control+Shift+z');
    await waitValue(ctx, NAME, 'Ember Lance');
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Ember Lance')).toBeVisible();
    await waitFor(ctx, (_: null) => /Spells \(1\)/.test(document.querySelector('[aria-label="Element kinds"]')?.textContent ?? ''), null, 'Spells (1)');
    expectEqual('status after save', "Saved 'Ember Lance' to Ember Isles.", await statusMsg(ctx));

    // The live description pane shows the element as the builder would.
    await expect(ctx.screen.getByRole('complementary', 'Description').getByRole('heading', 'Ember Lance')).toBeVisible();
  });

  test('PROBLEMS: an emptied name is a problem on the field and in the panel; its row jumps to the field', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/characters');
    await makePack(ctx);
    await clickCss(ctx, '[aria-label="Homebrew packs"] [role=row][aria-selected]');
    await ctx.screen.getByRole('button', 'Open pack').tap();
    await ctx.screen.getByRole('button', 'New element').first().tap();
    await ctx.browser.locator('#hb-new-name').fill('Ember Bolt');
    await ctx.screen.getByRole('button', 'Create').tap();
    await waitValue(ctx, NAME, 'Ember Bolt');

    await ctx.browser.locator(NAME).fill('');
    await expect(ctx.screen.getByRole('button', /Problems \(1\)/)).toBeVisible();
    expectEqual('status line', '1 problem', await statusMsg(ctx));
    await ctx.screen.getByRole('button', /Problems \(1\)/).tap();
    await ctx.screen.getByRole('button', 'Give it a name.').tap();
    await waitFor(ctx, (sel: string) => document.activeElement === document.querySelector(sel), NAME, 'focus on the name field');
  });

  test('switch off (no ?creator=1): no HOMEBREW sub-tab, and /homebrew goes back to the characters', async (ctx) => {
    await bootToHome(ctx, '/#/characters');
    await expect(ctx.screen.getByRole('link', 'Notes')).toBeVisible();
    await expect(ctx.screen.getByRole('link', 'Homebrew')).toBeHidden();
    await ctx.app.open('/#/homebrew');
    await expect(ctx.screen.getByRole('heading', 'Characters')).toBeAttached();
    await expect(ctx.screen.getByRole('grid', 'Homebrew packs')).toBeHidden();
  });
});
