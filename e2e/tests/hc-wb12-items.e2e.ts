import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual, settledBox } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { dblClickCss, textOf, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-12 (spec 4.9, 13.4, 13.5): the item forms on the web mock with the creator switched on (`?creator=1`),
// desktop 1355x847. A weapon with damage dice and a mastery picked with the reference picker, saved; then a magic item
// with a rarity and a +1 bonus, saved; both are in the pack's element list. Deterministic: no model.

const NAME = '#hb-f-name-input';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

/** Scrolls an element into view and waits until it holds still (the hit-tested click helpers test its centre). */
async function reveal(ctx: Ctx, css: string): Promise<void> {
  await ctx.browser.evaluate((sel: string) => {
    const el = document.querySelector(sel);
    el?.scrollIntoView({ block: 'center' });
    return el !== null;
  }, css);
  await settledBox(ctx.browser.locator(css).first());
}

/** Types into the reference picker's search and waits for its answer (the query goes out once typing pauses). */
async function searchPicker(ctx: Ctx, text: string): Promise<void> {
  await ctx.browser.locator('.hb-picker .pick-fsearch').fill(text);
  await waitFor(ctx, (want: string) => {
    const rows = [...document.querySelectorAll('.hb-pick-list .pick-row')];
    return rows.length > 0 && rows.every((r) => (r.textContent ?? '').toLowerCase().includes(want));
  }, text.toLowerCase(), `picker rows matching "${text}"`);
}

/** New element of a kind (its card in the New element flyout) with a name. `button`: the empty pack's "New element",
 * or the element list's "New" once an element is open. */
async function newElement(ctx: Ctx, button: string, kind: RegExp, name: string): Promise<void> {
  await ctx.screen.getByRole('button', button).first().tap();
  await ctx.browser.locator('#hb-new-name').fill(name);
  await ctx.screen.getByRole('radio', kind).tap();
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, name);
}

describe('hc-wb12 homebrew item forms (web mock)', { tags: ['homebrew'] }, () => {
  test('desktop: a weapon with a mastery and a magic item with a bonus, saved to the pack', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Forge of Embers');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Forge of Embers')).toBeAttached();

    // Weapon: damage dice, then the mastery from the reference picker (Mastery Property rows, several allowed: OK).
    await newElement(ctx, 'New element', /^Weapon/, 'Sickle Sword');
    await expect(ctx.screen.getByRole('heading', 'Weapon')).toBeVisible();
    await ctx.browser.locator('#hb-f-damage-input').fill('1d6');
    await reveal(ctx, '[data-field="mastery"] .hb-pick-btn');
    await ctx.browser.locator('[data-field="mastery"] .hb-pick-btn').click();
    await expect(ctx.screen.getByRole('dialog', /^Choose/)).toBeVisible();
    await searchPicker(ctx, 'sap');
    const sap = '.hb-pick-list .pick-row[data-row="Sap"] button.pick-main';
    await settledBox(ctx.browser.locator(sap).first());
    await dblClickCss(ctx, sap);
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('dialog', /^Choose/)).toBeHidden();
    expectEqual('the mastery by name', 'Sap', await textOf(ctx, '[data-field="mastery"] .hb-ref-name'));
    await autosaved(ctx);
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Sickle Sword')).toBeVisible();

    // Magic item: rarity and a +1 bonus; the live description pane lists both.
    await newElement(ctx, 'New', /^Magic item/, 'Ember Blade');
    await ctx.browser.locator('#hb-f-rarity-input').fill('Rare');
    await reveal(ctx, '[data-field="enhancement"]');
    await ctx.screen.getByRole('button', 'Increase Bonus (+N)').tap();
    await autosaved(ctx);
    await waitFor(ctx, (_: null) => {
      const pane = document.querySelector('.hb-ws-desc')?.textContent ?? '';
      return pane.includes('Rare') && pane.includes('+1');
    }, null, 'preview rarity and bonus');
    await pressKey(ctx, 'Control+s');

    // Both are in the pack: CATEGORIES counts one of each, and each kind's list holds its element.
    await expect(ctx.screen.getByRole('button', 'Ember Blade')).toBeVisible();
    await expect(ctx.screen.getByRole('button', 'Magic items (1)')).toBeVisible();
    await ctx.screen.getByRole('button', 'Weapons (1)').tap();
    await expect(ctx.screen.getByRole('button', 'Sickle Sword')).toBeVisible();
  });
});
