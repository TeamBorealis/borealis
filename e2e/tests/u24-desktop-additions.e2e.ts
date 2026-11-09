import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { clickSubTab, dblClickCss, textOf, waitStatus, type Ctx } from '../support/flows-a.js';

// U24: three tester round B additions reach desktop, drawn with Aurora's own pieces (U7): B5 the background ability
// option's scores in the description pane, B8 the granted feats on Feats as locked chosen-pick lines, B11 "Suggested for
// <class>" under the generation method with an Apply button (never automatic). Desktop keeps Aurora's status wording (B7).
// Seeded: Seraphine Vale, a 2024 Sage wizard (origin feat Skilled). `?mockAbilityOptions=1` makes the mock offer a 2024
// background's ability increases as the engine does: one list of options ("Intelligence +2, Wisdom +1").
// Deterministic: no model.

async function openSeraphine(ctx: Ctx, query = ''): Promise<void> {
  await ctx.app.open(`/${query}#/characters`);
  await expect(ctx.screen.getByRole('menuitem', 'File')).toBeVisible();
  await dblClickCss(ctx, '.v4-tile-open[aria-label="Seraphine Vale, Level 2 Elf Wizard"]');
  await waitStatus(ctx, (s) => s.startsWith('Loaded'));
}

describe('u24 desktop additions', { tags: ['u24'] }, () => {
  test('B5: a background ability option shows each score and modifier before and after in the description pane', async (ctx) => {
    await openSeraphine(ctx, '?mockAbilityOptions=1');
    await clickSubTab(ctx, 'Background');
    await ctx.screen.getByRole('button', /^Intelligence \+2, Wisdom \+1/).click();
    const pane = ctx.screen.getByRole('complementary', 'Description');
    await expect(pane.getByRole('heading', 'Intelligence +2, Wisdom +1')).toBeVisible();
    await expect(pane.getByText('An option of Ability Scores (Sage). Your scores with it:')).toBeVisible();
    // Seraphine has INT 15 and WIS 12 before her background's increases (none chosen in this scenario).
    const rows = await ctx.browser.evaluate((_: null) => Array.from(document.querySelectorAll('.bp-right .abil-opt-row'))
      .map((r) => Array.from(r.children).map((c) => (c.textContent ?? '').trim())), null);
    expect(rows).toEqual([['Intelligence', '15 → 17', '+2 → +3'], ['Wisdom', '12 → 13', '+1 → +1']]);
    expect(await ctx.browser.evaluate((_: null) => getComputedStyle(document.querySelector('.bp-right .abil-opt-row dt')!, '::after').content, null)).toBe('"."');
  });

  test('B8: Feats lists the granted origin feat read-only, and its name describes it', async (ctx) => {
    await openSeraphine(ctx);
    await clickSubTab(ctx, 'Feats');
    const list = ctx.screen.getByRole('list', 'Granted feats');
    await expect(list).toBeVisible();
    await expect(list.getByText('from Sage')).toBeVisible();
    await expect(list.getByRole('button', /^(Remove|Clear)/)).toBeHidden();
    await list.getByRole('button', 'Skilled').click();
    await expect(ctx.screen.getByRole('complementary', 'Description').getByRole('heading', 'Skilled')).toBeVisible();
    // Desktop keeps Aurora's status wording (B7): the phone's "Next: choose ..." never shows here.
    expect(await textOf(ctx, 'footer.aur-status .status-msg')).not.toMatch(/^Next: choose/);
  });

  test('B11: Ability Scores suggests the class standard array under the method row; Apply sets it', async (ctx) => {
    await openSeraphine(ctx);
    await clickSubTab(ctx, 'Ability Scores');
    const card = ctx.screen.getByRole('region', 'Suggested for Wizard');
    await expect(card).toBeVisible();
    const scores = (await textOf(ctx, '.abil-suggest-scores')).replace(/\s+/g, ' ').trim();
    expect(scores).toMatch(/^STR \d+ DEX \d+ CON \d+ INT \d+ WIS \d+ CHA \d+$/);
    await card.getByRole('button', 'Apply').click();
    await expect(card.getByRole('button', 'Applied')).toBeVisible();
    // The base score beside each tile (an input under Manual) now holds the suggested value, in sheet order.
    const bases = await ctx.browser.evaluate((_: null) => Array.from(document.querySelectorAll('.abil-row .abil'))
      .map((a) => { const b = a.querySelector('.abil-base-input, .abil-base'); return b instanceof HTMLInputElement ? b.value : (b?.textContent ?? '').trim(); }), null);
    expect(bases).toEqual(Array.from(scores.matchAll(/\d+/g), (m) => m[0]));
  });
});
