import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual, settledBox } from '../support/index.js';
import { clickCss, dblClickCss, textOf, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-21 (spec 9, 13.9, 16): Try it on the web mock with the creator switched on (`?creator=1`). A feat with a rule
// that changes walking speed -> Try it -> setup (the feat in its slot) -> a choice made with the Build picker -> the level
// ladder steps the test character to 4, where the feat arrives and its speed shows marked as changed -> the LEVEL spinner
// -> close. Then the same on a phone (`?mock=mobile`, 392x850): the full-height sheet and one-line ladder rows.
// Deterministic: no model.

const NAME = '#hb-f-name-input';
const PHONE = { width: 392, height: 850 } as const;

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

/**
 * Brings an element of the flyout's scrolling body into view and waits until it holds still: the click helpers test what
 * is under its centre, and right after Try it opens the panel is still sliding in (the phone sheet rises for 300 ms) and
 * the Choices lists above may still be loading.
 */
async function reveal(ctx: Ctx, css: string): Promise<void> {
  await ctx.browser.evaluate((sel: string) => {
    const el = document.querySelector(sel);
    el?.scrollIntoView({ block: 'center' });
    return el !== null;
  }, css);
  await settledBox(ctx.browser.locator(css).first());
}

const trialLevel = (ctx: Ctx, n: number): Promise<void> =>
  waitFor(ctx, (want: number) => (document.querySelector('.hb-try-status')?.textContent ?? '').includes(`level ${want}`), n, `test character at level ${n}`);

/** NEW PACK "Ember Isles" -> New element: a Feat "Fleet Step" with the rule "Changes a number" on walking speed. */
async function newFeat(ctx: Ctx, phone: boolean): Promise<void> {
  await ctx.screen.getByRole('button', 'New pack').first().tap();
  await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
  await ctx.screen.getByRole('button', 'Create').tap();
  await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();
  await ctx.screen.getByRole('button', phone ? 'New' : 'New element').first().tap();
  await ctx.browser.locator('#hb-new-name').fill('Fleet Step');
  await ctx.screen.getByRole('radio', /^Feat/).tap();
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, 'Fleet Step');
  if (phone) return;
  await ctx.screen.getByRole('tab', 'RULES').tap();
  await ctx.screen.getByRole('button', 'Add rule').tap();
  await ctx.screen.getByRole('button', 'Changes a number').tap();
  await ctx.screen.getByRole('button', 'Pick: Statistic').tap();
  await ctx.browser.locator('input[aria-label="Search statistics"]').fill('walking');
  await ctx.screen.getByRole('button', /^Walking speed$/).tap();
  await ctx.screen.getByRole('button', 'OK').tap();
  await expect(ctx.screen.getByRole('listitem', /Changes a number \(Walking speed/)).toBeVisible();
  await autosaved(ctx);
}

describe('hc-wb21 homebrew Try it (web mock)', { tags: ['homebrew'] }, () => {
  test('desktop: open Try it from a feat, make a choice, step the ladder to 4, see the feat and its speed marked', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await newFeat(ctx, false);

    // Try it: the Level up flyout's 400 px panel with the test character at level 1.
    await ctx.screen.getByRole('button', 'Try it').tap();
    await expect(ctx.screen.getByRole('dialog', 'Try it')).toBeVisible();
    await trialLevel(ctx, 1);
    expectEqual('the draft in its slot', 'Fleet Step', await textOf(ctx, '.hb-try-draft-name'));
    await expect(ctx.screen.getByText(/first feat slot/)).toBeVisible();

    // CHOICES: the Build picker on the test character; Athletics is taken through the trial.
    await reveal(ctx, '.hb-try .pick-row[data-row="Athletics"] button.pick-main');
    await dblClickCss(ctx, '.hb-try .pick-row[data-row="Athletics"] button.pick-main');
    await waitFor(ctx, (_: null) => [...document.querySelectorAll('.hb-try .pick-chosen-name')].some((n) => n.textContent === 'Athletics'), null, 'Athletics chosen');
    await waitFor(ctx, (_: null) => /Athletics/.test(document.querySelector('.hb-try [aria-label="Skills"]')?.textContent ?? ''), null, 'Athletics in the summary');

    // LEVEL LADDER: level 4 -> the feat arrives (ladder row selected, the feature and its speed marked as changed).
    await expect(ctx.screen.getByRole('table', 'Level ladder')).toBeVisible();
    await reveal(ctx, '.hb-try-ladder tr[data-level="4"] button.ap-name');
    await clickCss(ctx, '.hb-try-ladder tr[data-level="4"] button.ap-name');
    await trialLevel(ctx, 4);
    await waitFor(ctx, (_: null) => document.querySelector('.hb-try-ladder tr[data-level="4"]')?.classList.contains('sel') === true, null, 'level 4 row selected');
    await waitFor(ctx, (_: null) => [...document.querySelectorAll('.hb-try-changed')].some((n) => /31 ft\./.test(n.textContent ?? '')), null, 'speed marked');
    await waitFor(ctx, (_: null) => [...document.querySelectorAll('.hb-try [aria-label="Features"] .hb-try-changed')].some((n) => /Fleet Step/.test(n.textContent ?? '')), null, 'feat marked');
    await expect(ctx.screen.getByText(/Taken at level 4/)).toBeVisible();

    // The LEVEL spinner steps on from there.
    await reveal(ctx, '.hb-try-setup .aur-numeric');
    await ctx.screen.getByRole('button', 'Increase Level').tap();
    await trialLevel(ctx, 5);

    await ctx.screen.getByRole('button', 'Close').last().tap();
    await expect(ctx.screen.getByRole('dialog', 'Try it')).toBeHidden();
    await expect(ctx.screen.getByRole('region', 'Element editor')).toBeVisible();
  });

  test('phone: Try it is a full-height sheet; the ladder rows are one line with the detail under the level', async (ctx) => {
    await ctx.browser.setViewport(PHONE);
    await ctx.app.open('/?creator=1&mock=mobile#/homebrew');
    await expect(ctx.screen.getByRole('button', 'New pack').first()).toBeVisible();
    await newFeat(ctx, true);
    await autosaved(ctx);
    await ctx.screen.getByRole('button', 'Try it').last().tap();
    await expect(ctx.screen.getByRole('dialog', 'Try it')).toBeVisible();
    await trialLevel(ctx, 1);
    await waitFor(ctx, (_: null) => document.querySelector('.flyout .sheet-grabber') !== null, null, 'sheet grabber');
    await waitFor(ctx, (_: null) => /Fleet Step/.test(document.querySelector('.hb-try-ladder tr[data-level="4"] .ph-row-detail')?.textContent ?? ''), null, 'ladder detail line');
    await reveal(ctx, '.hb-try-ladder tr[data-level="4"] button.ap-name');
    await clickCss(ctx, '.hb-try-ladder tr[data-level="4"] button.ap-name');
    await trialLevel(ctx, 4);
    await ctx.screen.getByRole('button', 'Close').last().tap();
    await expect(ctx.screen.getByRole('dialog', 'Try it')).toBeHidden();
  });
});
