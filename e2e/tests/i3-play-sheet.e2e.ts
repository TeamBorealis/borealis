import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { clickCss, clickMainTab, clickSubTab, loadSeraphine, textOf, waitFor } from '../support/flows-a.js';
import type { Ctx } from '../support/flows-a.js';

// GitHub #14 (Tjop Vaoxad) and #1 (kirktobridge), decision U16(b): the Play tab as the table-session sheet. Seeded
// Seraphine Vale (2024 Wizard 2). A tap rolls a check, a right-click offers advantage, the roll shows in the result card
// and in Recent rolls; Cast on Magic Missile spends one of her three level 1 slots; a spell's name opens its description
// in the pane. Real pointer clicks, mock host, deterministic checks (the dice are random, so only their shape is read).
const RESULT = '[role=status][aria-label="Roll result"]';

async function clickInView(ctx: Ctx, css: string): Promise<void> {
  await ctx.browser.evaluate((css: string) => { document.querySelector(css)?.scrollIntoView({ block: 'center' }); return true; }, css);
  await clickCss(ctx, css);
}

const resultHas = (ctx: Ctx, re: string, what: string) =>
  waitFor(ctx, (re: string) => new RegExp(re).test(document.querySelector('[role=status][aria-label="Roll result"]')?.textContent ?? ''), re, what);

describe('#14 play sheet', { tags: ['i3'] }, () => {
  test('a tap rolls, a right-click rolls with advantage, Cast spends a slot and a spell name opens its description', async (ctx) => {
    await loadSeraphine(ctx);
    await clickMainTab(ctx, 'Play');
    await waitFor(ctx, (_: null) => location.hash.startsWith('#/play'), null, 'route /play');

    await clickSubTab(ctx, 'Abilities');
    await clickInView(ctx, 'button[aria-label^="Roll Strength check,"]');
    await resultHas(ctx, '^Strength check -?\\d+\\. d20 \\[\\d+\\] [+-]\\d+ = -?\\d+', 'a check result');

    const stealth = 'button[aria-label^="Roll Stealth check,"]';
    // the right-click's contextmenu event (the engine's tap has no right button)
    await ctx.browser.evaluate((css: string) => {
      const el = document.querySelector(css);
      el?.scrollIntoView({ block: 'center' });
      const r = el?.getBoundingClientRect();
      el?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2, clientX: r?.x, clientY: r?.y }));
      return true;
    }, stealth);
    await expect(ctx.screen.getByRole('menu', 'Roll Stealth check')).toBeVisible();
    await clickCss(ctx, '[role=menu] [role=menuitem]:first-child');
    await resultHas(ctx, '^Stealth check -?\\d+\\. d20 \\[\\d+, \\d+\\] [+-]\\d+ = -?\\d+, advantage', 'an advantage result');

    await clickInView(ctx, '.pr-bar button');
    await expect(ctx.screen.getByRole('dialog', 'Recent rolls')).toBeVisible();
    expect(await ctx.browser.evaluate((_: null) => document.querySelectorAll('[aria-label="Recent rolls"] li').length, null)).toBe(2);
    await clickCss(ctx, '.dialog-foot .btn-primary');

    await clickSubTab(ctx, 'Spells');
    await clickInView(ctx, 'button[aria-label="Cast Magic Missile, level 1 slot"]');
    await waitFor(ctx, (_: null) => (document.querySelector('.pr-level-slots')?.textContent ?? '').includes('2/3 slots'), null, 'a slot spent');
    await clickInView(ctx, 'button[aria-label="Magic Missile, description"]');
    await waitFor(ctx, (_: null) => (document.querySelector('aside[aria-label="Spells details"]')?.textContent ?? '').includes('Level 1 Evocation'), null, 'the spell description');
    expect(await textOf(ctx, RESULT)).not.toContain('Magic Missile');
  });
});
