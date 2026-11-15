import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { clickMainTab, loadSeraphine, statusMsg, waitFor, type Ctx } from '../support/flows-a.js';

// HC1 (items HtCY8, HtCaM) on the web mock, desktop 1355x847. A Skill made in the creator is a character skill, so a
// name the sheet can't use (an ability's) is refused at the name until it is renamed; then it saves. On the Play tab,
// the Conditions card lists the conditions under Exhaustion: Prone switched on is tagged Active, and its row opens its
// text in the description pane. Deterministic: no model.

const NAME = '#hb-f-name-input';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const fieldMessage = (ctx: Ctx, key: string): Promise<string> =>
  ctx.browser.evaluate((k: string) => document.querySelector(`[data-field="${k}"] .hb-field-msg`)?.textContent ?? '', key);

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

describe('hc-next-engine homebrew skills and switched conditions (web mock)', { tags: ['homebrew'] }, () => {
  test('a skill named like an ability is refused at its name; renamed, it saves', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Old World');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Old World')).toBeAttached();
    await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
    await ctx.screen.getByRole('radio', /^Skill A skill/).tap();
    await ctx.browser.locator('#hb-new-name').fill('Strength');
    await ctx.screen.getByRole('button', 'Create').tap();
    await waitValue(ctx, NAME, 'Strength');
    await ctx.browser.locator('#hb-f-ability-input').fill('Intelligence');
    await waitFor(ctx, (_: null) => /colon: rename it\./.test(document.querySelector('[data-field="name"] .hb-field-msg')?.textContent ?? ''), null, 'the name refused');
    expectEqual('name sentence', "A skill can't be named like an ability or another statistic (Strength, AC, Level), or hold a colon: rename it.", await fieldMessage(ctx, 'name'));
    await ctx.browser.locator(NAME).fill('Alchemy');
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await autosaved(ctx);
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Alchemy')).toBeVisible();
    expectEqual('status after the skill', "Saved 'Alchemy' to Old World", await statusMsg(ctx));
  });

  test('the Conditions card switches Prone on and shows its text', async (ctx) => {
    await loadSeraphine(ctx);
    await clickMainTab(ctx, 'Play');
    await waitFor(ctx, (_: null) => location.hash.startsWith('#/play'), null, 'route /play');
    await waitFor(ctx, (_: null) => document.querySelectorAll('ul[aria-label="Conditions"] li').length === 14, null, 'fourteen conditions');
    await ctx.screen.getByRole('switch', 'Prone').tap();
    await waitFor(ctx, (_: null) => {
      const row = [...document.querySelectorAll('ul[aria-label="Conditions"] li')].find((li) => li.querySelector('span.grow')?.textContent === 'Prone');
      return (row?.querySelector('input') as HTMLInputElement | null)?.checked === true && /Active/.test(row?.textContent ?? '');
    }, null, 'Prone on and Active');
    await ctx.screen.getByRole('button', /^Prone/).tap();
    await waitFor(ctx, (_: null) => /Your only movement option is to crawl\./.test(document.body.textContent ?? ''), null, 'the description');
  });
});
