import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { clickByText, clickCss, clickMainTab, loadSeraphine, textOf, waitFor } from '../support/flows-a.js';
import type { Ctx } from '../support/flows-a.js';

// GitHub #2 (decision U13): the Play tab spell tracker. Seeded Seraphine Vale (2024 Wizard 2, three 1st level slots, Detect
// Magic in her spellbook). Mark a slot used, a long rest brings it back; concentrate on Detect Magic (pinned at the top of
// Play, one tap ends it); add a custom tracker and track it. Real pointer clicks, mock host, deterministic: no model.
const L1_STATUS = '[role=status][aria-label^="Level 1 slots:"]';
const PINNED = 'section[aria-label="Concentration"]';
const INFECTION_STATUS = '[role=status][aria-label^="Infection:"]';

/** Scroll `css` to the middle of its scroller, then click it with the real pointer (hit-tested). */
async function clickInView(ctx: Ctx, css: string): Promise<void> {
  await ctx.browser.evaluate((css: string) => { document.querySelector(css)?.scrollIntoView({ block: 'center' }); return true; }, css);
  await clickCss(ctx, css);
}

const textIs = (ctx: Ctx, css: string, want: string, what: string) =>
  waitFor(ctx, ({ css, want }: { css: string; want: string }) => (document.querySelector(css)?.textContent ?? '') === want, { css, want }, what);

async function openPlay(ctx: Ctx) {
  await loadSeraphine(ctx);
  await clickMainTab(ctx, 'Play');
  await waitFor(ctx, (_: null) => location.hash.startsWith('#/play'), null, 'route /play');
  await waitFor(ctx, (css: string) => !!document.querySelector(css), L1_STATUS, 'spell slots card');
}

describe('#2 spell tracker', { tags: ['i2'] }, () => {
  test('a used slot comes back on a long rest; concentration is pinned and ended; a custom tracker is tracked', async (ctx) => {
    await openPlay(ctx);
    expect(await textOf(ctx, L1_STATUS)).toBe('3/3');

    await clickInView(ctx, 'button[aria-label="Level 1 slot 1 of 3, available"]');
    await textIs(ctx, L1_STATUS, '2/3', 'one slot used');
    expect(await ctx.browser.evaluate((_: null) => document.querySelector('button[aria-label="Level 1 slot 3 of 3, used"]')?.getAttribute('aria-pressed') ?? '', null)).toBe('true');
    await clickInView(ctx, '.aur-expander-extra button.btn-primary');
    await textIs(ctx, L1_STATUS, '3/3', 'slots back after the long rest');

    // Concentration: picked from her concentration spells, pinned above the stat tiles, one tap ends it.
    await ctx.browser.evaluate((_: null) => { document.querySelector('select[aria-label="Concentrate on"]')?.scrollIntoView({ block: 'center' }); return true; }, null);
    await ctx.browser.locator('select[aria-label="Concentrate on"]').selectOption({ label: 'Detect Magic (level 1)' });
    await waitFor(ctx, (css: string) => (document.querySelector(css)?.textContent ?? '').includes('Detect Magic'), PINNED, 'pinned concentration');
    expect(await ctx.browser.evaluate((css: string) => {
      const pinned = document.querySelector(css);
      const tile = document.querySelector('.v5-list-pane .stat-row');
      return !!pinned && !!tile && !!(pinned.compareDocumentPosition(tile) & Node.DOCUMENT_POSITION_FOLLOWING);
    }, PINNED)).toBe(true);
    await clickInView(ctx, 'button[aria-label="End concentration on Detect Magic"]');
    await waitFor(ctx, (css: string) => !document.querySelector(css), PINNED, 'concentration ended');

    // A custom tracker: added by hand, tracked with its pips, kept by a long rest (it never resets).
    await clickInView(ctx, 'button[aria-label="Add tracker"]');
    await expect(ctx.screen.getByRole('dialog', 'Add tracker')).toBeVisible();
    await ctx.browser.locator('.st-form input:not([type=number])').fill('Infection');
    await ctx.browser.locator('.st-form input[type=number]').fill('6');
    await ctx.browser.locator('.st-form select').selectOption({ value: 'never' });
    await clickByText(ctx, '.dialog-foot button', 'Save');
    await waitFor(ctx, (css: string) => !!document.querySelector(css), INFECTION_STATUS, 'tracker row');
    expect(await textOf(ctx, INFECTION_STATUS)).toBe('6/6');
    await clickInView(ctx, 'button[aria-label="Infection 1 of 6, available"]');
    await textIs(ctx, INFECTION_STATUS, '5/6', 'one pip marked');
    await clickInView(ctx, '.aur-expander-extra button.btn-primary');
    await textIs(ctx, L1_STATUS, '3/3', 'rested');
    expect(await textOf(ctx, INFECTION_STATUS)).toBe('5/6');
  });
});
