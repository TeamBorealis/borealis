import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { clickByText, clickCss, clickMainTab, loadSeraphine, textOf, waitFor } from '../support/flows-a.js';
import type { Ctx } from '../support/flows-a.js';

// GitHub #2 (decision U13): the Play tab spell tracker. Seeded Seraphine Vale (2024 Wizard 2, three 1st level slots, Detect
// Magic in her spellbook). Mark slots used (two quick presses both count), the Long Rest in the page's rest bar brings them
// back; concentrate on Detect Magic (pinned at the top of Play, one tap ends it); add a counting-up tracker, track it, and
// delete it in two presses. Real pointer clicks, mock host, deterministic: no model.
const L1_STATUS = '[role=status][aria-label^="Level 1 slots:"]';
const PINNED = 'section[aria-label="Concentration"]';
const INFECTION_STATUS = '[role=status][aria-label^="Infection:"]';
const LONG_REST = '[role=group][aria-label="Rests"] button.btn-primary';
const L1_USE = 'button[aria-label^="Use a level 1 slot,"]';

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
  test('used slots come back on a long rest; concentration is pinned and ended; a counting-up tracker is tracked and deleted', async (ctx) => {
    await openPlay(ctx);
    expect(await textOf(ctx, L1_STATUS)).toBe('3/3');
    // the rests are a bar above the concentration bar and the stat tiles
    expect(await ctx.browser.evaluate((_: null) => {
      const bar = document.querySelector('[role=group][aria-label="Rests"]');
      const tile = document.querySelector('.v5-list-pane .stat-row');
      return !!bar && !!tile && !!(bar.compareDocumentPosition(tile) & Node.DOCUMENT_POSITION_FOLLOWING);
    }, null)).toBe(true);

    await clickInView(ctx, L1_USE);
    await clickInView(ctx, L1_USE);
    await textIs(ctx, L1_STATUS, '1/3', 'two slots used');
    await clickInView(ctx, LONG_REST);
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

    // A counting-up tracker: added by hand, gains a pip, kept by a long rest (it never resets), deleted in two presses.
    await clickInView(ctx, 'button[aria-label="Add tracker"]');
    await expect(ctx.screen.getByRole('dialog', 'Add tracker')).toBeVisible();
    await ctx.browser.locator('.st-form input[type=text], .st-form input:not([type])').first().fill('Infection');
    await ctx.browser.locator('.st-form input[type=number]').fill('6');
    await ctx.browser.locator('.st-form select').selectOption({ value: 'never' });
    await ctx.browser.locator('.st-form input[type=checkbox]').check();
    await clickByText(ctx, '.dialog-foot button', 'Save');
    await waitFor(ctx, (css: string) => !!document.querySelector(css), INFECTION_STATUS, 'tracker row');
    expect(await textOf(ctx, INFECTION_STATUS)).toBe('0/6');
    await clickInView(ctx, 'button[aria-label="Add one to Infection, 0 of 6"]');
    await textIs(ctx, INFECTION_STATUS, '1/6', 'one gained');
    await clickInView(ctx, LONG_REST);
    await textIs(ctx, L1_STATUS, '3/3', 'rested');
    expect(await textOf(ctx, INFECTION_STATUS)).toBe('1/6');

    await clickInView(ctx, 'button[aria-label="Edit Infection"]');
    await expect(ctx.screen.getByRole('dialog', 'Edit tracker')).toBeVisible();
    await clickByText(ctx, '.dialog-foot button', 'Delete');
    await waitFor(ctx, (_: null) => Array.from(document.querySelectorAll('.dialog-foot button')).some((b) => b.textContent === 'Confirm delete'), null, 'confirm step');
    expect(await ctx.browser.evaluate((css: string) => !!document.querySelector(css), INFECTION_STATUS)).toBe(true);
    await clickByText(ctx, '.dialog-foot button', 'Confirm delete');
    await waitFor(ctx, (css: string) => !document.querySelector(css), INFECTION_STATUS, 'tracker deleted');
  });
});
