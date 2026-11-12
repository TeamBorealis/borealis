import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { clickCss, dblClickCss, statusMsg, waitFor, type Ctx } from '../support/flows-a.js';
import { LOAD_MS, waitPackList, waitValue, waitWorkspace } from '../support/creator.js';

// Homebrew WB-09 (spec 2, 10.2, 12.2, 13.1-13.3): START > HOMEBREW on the web mock with the creator switched on
// (`?creator=1`). Open a pack -> its workspace -> edit an element -> undo. And the switch: with `?creator=0` (on is the
// default from 1.5.0) the HOMEBREW sub-tab is not there and its route goes back to the characters. Deterministic: no model.

const NAME = '#hb-f-name-input';

const inputValue = (ctx: Ctx, css: string): Promise<string> =>
  ctx.browser.evaluate((sel: string) => (document.querySelector(sel) as HTMLInputElement | null)?.value ?? '', css);

/** START > HOMEBREW > NEW PACK "Ember Isles", then back to the pack list. */
async function makePack(ctx: Ctx): Promise<void> {
  await ctx.screen.getByRole('link', 'Homebrew').tap();
  // the creator's routes load lazily: wait on the rendered screen, not a fixed budget (HvB30, slow under load)
  await waitPackList(ctx);
  await ctx.screen.getByRole('button', 'New pack').tap();
  await expect(ctx.screen.getByRole('dialog', 'New pack')).toBeVisible();
  await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
  expectEqual('code suggested from the name', 'EI', await inputValue(ctx, '#hb-pack-code'));
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitWorkspace(ctx, 'Ember Isles');
  await ctx.screen.getByRole('button', 'All packs').tap();
  await waitPackList(ctx);
}

describe('hc-wb09 homebrew creator shell (web mock)', { tags: ['homebrew'] }, () => {
  test('open pack -> workspace -> edit -> undo (button and Ctrl+Z), redo, save', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/characters');
    await makePack(ctx);

    // Open the pack from the grid (double-click, as the Sources grid opens).
    await dblClickCss(ctx, '[aria-label="Homebrew packs"] [role=row][aria-selected]');
    await waitWorkspace(ctx, 'Ember Isles');
    await expect(ctx.screen.getByRole('navigation', 'Element kinds')).toBeVisible();

    // New element: a spell named Ember Bolt.
    await ctx.screen.getByRole('button', 'New element').first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
    await ctx.browser.locator('#hb-new-name').fill('Ember Bolt');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('region', 'Element editor')).toBeVisible();
    await waitValue(ctx, NAME, 'Ember Bolt', LOAD_MS);

    // Edit: the name, then a combo (a separate undo step).
    await ctx.browser.locator(NAME).fill('Ember Lance');
    await ctx.browser.locator('#hb-f-school-input').selectOption('Evocation');
    await waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

    // Undo by the button (the school), then by Ctrl+Z (the name).
    await clickCss(ctx, '.hb-commands button[aria-label="Undo"]');
    expectEqual('school undone', '', await inputValue(ctx, '#hb-f-school-input'));
    expectEqual('name kept after one undo', 'Ember Lance', await inputValue(ctx, NAME));
    await pressKey(ctx, 'Control+z');
    await waitValue(ctx, NAME, 'Ember Bolt', LOAD_MS);

    // Redo by Ctrl+Shift+Z, then Ctrl+S saves the element (on this route it never saves a character).
    await pressKey(ctx, 'Control+Shift+z');
    await waitValue(ctx, NAME, 'Ember Lance');
    // A second redo brings the school back: a spell needs one to be saved (WB-11).
    await pressKey(ctx, 'Control+Shift+z');
    await waitValue(ctx, '#hb-f-school-input', 'Evocation');
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Ember Lance')).toBeVisible();
    await waitFor(ctx, (_: null) => /Spells \(1\)/.test(document.querySelector('[aria-label="Element kinds"]')?.textContent ?? ''), null, 'Spells (1)');
    expectEqual('status after save', "Saved 'Ember Lance' to Ember Isles", await statusMsg(ctx));

    // The live description pane shows the element as the builder would.
    await expect(ctx.screen.getByRole('complementary', 'Description').getByRole('heading', 'Ember Lance')).toBeVisible();
  });

  test('PROBLEMS: an emptied name is a problem on the field and in the panel; its row jumps to the field', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/characters');
    await makePack(ctx);
    await clickCss(ctx, '[aria-label="Homebrew packs"] [role=row][aria-selected]');
    await ctx.screen.getByRole('button', 'Open pack').tap();
    await waitWorkspace(ctx, 'Ember Isles');
    await ctx.screen.getByRole('button', 'New element').first().tap();
    await ctx.browser.locator('#hb-new-name').fill('Ember Bolt');
    await ctx.screen.getByRole('button', 'Create').tap();
    await waitValue(ctx, NAME, 'Ember Bolt', LOAD_MS);
    // A new spell asks for its school (WB-11); choose it so the name is the one problem.
    await ctx.browser.locator('#hb-f-school-input').selectOption('Evocation');

    await ctx.browser.locator(NAME).fill('');
    await expect(ctx.screen.getByRole('button', /Problems \(1\)/)).toBeVisible();
    expectEqual('status line', '1 problem', await statusMsg(ctx));
    // PROBLEMS opened by itself for the school at first; open it if it is closed.
    const expanded = await ctx.browser.evaluate((_: null) => document.querySelector('.hb-problems [aria-expanded]')?.getAttribute('aria-expanded') ?? '', null);
    if (expanded !== 'true') await ctx.screen.getByRole('button', /Problems \(1\)/).tap();
    await ctx.screen.getByRole('button', 'Give it a name.').tap();
    await waitFor(ctx, (sel: string) => document.activeElement === document.querySelector(sel), NAME, 'focus on the name field');
  });

  test('switch off (?creator=0): no HOMEBREW sub-tab, and /homebrew goes back to the characters', async (ctx) => {
    await bootToHome(ctx, '/?creator=0#/characters');
    await expect(ctx.screen.getByRole('link', 'Notes')).toBeVisible();
    await expect(ctx.screen.getByRole('link', 'Homebrew')).toBeHidden();
    await ctx.app.open('/?creator=0#/homebrew');
    await expect(ctx.screen.getByRole('heading', 'Characters')).toBeAttached();
    await expect(ctx.screen.getByRole('grid', 'Homebrew packs')).toBeHidden();
  });
});
