import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { statusMsg, textOf, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-23 (spec 12.3, 12.4 on the web mock, desktop 1355x847, `?creator=1`): the pack's SPELL LISTS view. Choose
// the Wizard list, check three spells another pack owns (each becomes an additive "add to this list" append, HC5), save
// with Ctrl+S; the counter and the checks hold after the list is read again, and WHERE USED of one of them names the
// Wizard list. Deterministic: no model.

const GRID = '.hb-lists table[aria-label="Spells"]';
const SPELLS = ['Bless', 'Cure Wounds', 'Guidance'];

const counter = (ctx: Ctx): Promise<string> => textOf(ctx, '.hb-lists-count');
const checked = (ctx: Ctx, name: string): Promise<boolean> =>
  ctx.browser.evaluate(({ g, n }: { g: string; n: string }) => (document.querySelector(`${g} input[aria-label="Choose ${n}"]`) as HTMLInputElement | null)?.checked === true, { g: GRID, n: name });
const onList = (text: string): number => Number(/(\d+) spells on this list/.exec(text)?.[1] ?? NaN);

async function chooseList(ctx: Ctx, name: string): Promise<void> {
  await ctx.browser.locator('.hb-lists select[aria-label="Spell list"]').selectOption(name);
  await waitFor(ctx, (g: string) => document.querySelector(g) !== null, GRID, 'spell grid');
}

describe('hc-wb23 homebrew SPELL LISTS and where used (web mock)', { tags: ['homebrew'] }, () => {
  test('check three spells onto the Wizard list, save, and see the list and where used', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();

    await ctx.screen.getByRole('button', 'Spell Lists').tap();
    await expect(ctx.screen.getByRole('region', 'Spell lists')).toBeVisible();
    await chooseList(ctx, 'Wizard');
    const before = onList(await counter(ctx));
    expectEqual('a spell the Wizard list has in its own pack is locked', 'locked',
      await ctx.browser.evaluate((g: string) => (document.querySelector(`${g} input[aria-label="Choose Fireball"]`) as HTMLInputElement | null)?.disabled ? 'locked' : 'open', GRID));

    for (const name of SPELLS) await ctx.browser.locator(`${GRID} input[aria-label="Choose ${name}"]`).check();
    expectEqual('checks wait for Save', `${before + 3} spells on this list (3 not saved)`, await counter(ctx));

    await pressKey(ctx, 'Control+s');
    await waitFor(ctx, (want: string) => document.querySelector('.hb-lists-count')?.textContent === want, `${before + 3} spells on this list`, 'saved count');
    expectEqual('status after save', `Saved the Wizard list: ${before + 3} spells.`, await statusMsg(ctx));

    // read again: another list, then Wizard
    await chooseList(ctx, 'Cleric');
    await chooseList(ctx, 'Wizard');
    await waitFor(ctx, (want: string) => document.querySelector('.hb-lists-count')?.textContent === want, `${before + 3} spells on this list`, 'count read again');
    for (const name of SPELLS) expectEqual(`${name} is on the list`, 'checked', (await checked(ctx, name)) ? 'checked' : 'unchecked');

    // WHERE USED of Bless names the Wizard list (and Cleric, its own)
    await ctx.browser.evaluate((g: string) => {
      const b = [...document.querySelectorAll<HTMLButtonElement>(`${g} button.ap-name`)].find((x) => x.textContent === 'Bless');
      b?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      b?.click();
      return b !== undefined;
    }, GRID);
    await expect(ctx.screen.getByRole('region', 'Where used')).toBeVisible();
    await waitFor(ctx, (_: null) => [...document.querySelectorAll('.hb-where-list .ap-name')].some((b) => b.textContent === 'Wizard'), null, 'Wizard in where used');
    // each row: its name and what it does (a loaded element's row also has Edit, which edits a copy: P2b, HC5)
    const uses = await ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-where-list li')]
      .map((li) => `${li.querySelector('.ap-name')?.textContent ?? ''}${li.querySelector('.hb-where-what')?.textContent ?? ''}${li.querySelector('.hb-where-edit') ? '+Edit' : ''}`), null);
    expectEqual('where used', 'ClericLists it · Class+Edit|WizardLists it · Class+Edit', uses.join('|'));
  });
});
