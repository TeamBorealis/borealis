import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { expectEqual } from '../support/index.js';
import {
  clickByText, clickCss, dblClickByText, isDisabled, loadSeraphine, openLevelUp, rawTextOf, rowsText, style, textOf, waitFor, waitStatus,
} from '../support/flows-a.js';

// Level up, hit points, multiclass, remove last level.
// Seeded character: Seraphine Vale, level 2 Elf Wizard (MockApi). Deterministic: no model.
const FLYOUT = '.flyout';
const REMOVE = 'button.lvl-link.remove';
const REMOVE_COLOUR_ENABLED = 'rgb(230, 230, 230)'; // #E6E6E6
const REMOVE_COLOUR_DISABLED = 'rgb(108, 108, 109)'; // #6C6C6D at level 1

const profileLine = (ctx: Parameters<typeof rawTextOf>[0]) =>
  rawTextOf(ctx, '.aur-profile').then((t) => t.replace(/\s+/g, ' '));

describe('w2 level up', { tags: ['w2'] }, () => {
  test('flyout layout: title, XP bar, level line, class panel, multiclassing expander', async (ctx) => {
    const { screen } = ctx;
    await loadSeraphine(ctx);
    await openLevelUp(ctx);
    const text = (await textOf(ctx, FLYOUT)).replace(/\n+/g, ' / ');
    // Order: title, XP, "Your character is level N", remove link, class panel with LEVEL UP, Multiclassing.
    const order = ['LEVEL UP & MULTICLASSING', '300 XP', '900 XP', 'YOUR CHARACTER IS LEVEL 2', 'REMOVE THE LAST LEVEL', 'WIZARD (2)', 'LEVEL UP', 'MULTICLASSING'];
    let at = -1;
    for (const part of order) {
      const next = text.indexOf(part, at + 1);
      expect(next, `"${part}" after position ${at} in: ${text}`).toBeGreaterThan(at);
      at = next;
    }
    // The remove link is #E6E6E6 when enabled.
    expectEqual('remove-last-level colour (level 2, enabled)', REMOVE_COLOUR_ENABLED, await style(ctx, REMOVE, 'color'));
    await expect(screen.getByRole('button', 'Level up Wizard')).toBeVisible();
  });

  test('level up: status line, level counter, new pickers', async (ctx) => {
    await loadSeraphine(ctx);
    await openLevelUp(ctx);
    await clickCss(ctx, 'button[aria-label="Level up Wizard"]');
    // Flyout "Level up" gives "Your character is now level 3."
    expectEqual('level-up status', 'Your character is now level 3.', await waitStatus(ctx, (s) => s.startsWith('Your character is now level')));
    await waitFor(ctx, (_: null) => /LEVEL 3 ELF WIZARD/i.test(document.querySelector('.aur-profile')?.textContent ?? ''), null, 'profile level 3');
    expect(await profileLine(ctx)).toContain('Level 3 Elf Wizard');
    expect(await textOf(ctx, FLYOUT)).toContain('WIZARD (3)');
    expect(await textOf(ctx, FLYOUT)).toContain('YOUR CHARACTER IS LEVEL 3');
    // The XP bar moves to the level 3 thresholds.
    expect(await textOf(ctx, FLYOUT)).toContain('2700 XP');
  });

  test('remove last level asks first, names the class, and Cancel changes nothing', async (ctx) => {
    const { screen } = ctx;
    await loadSeraphine(ctx);
    await openLevelUp(ctx);
    await clickCss(ctx, 'button[aria-label="Level up Wizard"]');
    await waitStatus(ctx, (s) => s === 'Your character is now level 3.');
    await clickCss(ctx, REMOVE);
    await expect(screen.getByRole('dialog', 'Remove last level')).toBeVisible();
    expectEqual('confirm text', 'Remove the most recent level (Wizard)? Its selections will be dropped.',
      await ctx.browser.evaluate((_: null) => ((Array.from(document.querySelectorAll('[role=dialog]')).pop() as HTMLElement | undefined)?.innerText ?? '').split('\n').find((l) => l.startsWith('Remove the most')) ?? '', null));
    await clickByText(ctx, '[role=dialog] button', 'Cancel');
    await expect(screen.getByRole('dialog', 'Remove last level')).toBeHidden();
    expect(await textOf(ctx, FLYOUT)).toContain('YOUR CHARACTER IS LEVEL 3');
    // Yes: back to level 2.
    await clickCss(ctx, REMOVE);
    await clickByText(ctx, '[role=dialog] button', 'Remove level');
    expectEqual('remove status', 'Your character is now level 2.', await waitStatus(ctx, (s) => s === 'Your character is now level 2.'));
    expect(await textOf(ctx, FLYOUT)).toContain('WIZARD (2)');
  });

  test('remove-last-level link is greyed out at level 1', async (ctx) => {
    const { browser } = ctx;
    await ctx.app.open('/#/characters');
    await clickCss(ctx, 'button.aur-newchar');
    await browser.locator('#v4-name').fill('Level One');
    await clickByText(ctx, '.flyout button', 'Create character');
    await waitFor(ctx, (_: null) => location.hash === '#/builder/race', null, 'route /builder/race');
    await openLevelUp(ctx);
    expect(await isDisabled(ctx, REMOVE)).toBe(true);
    expectEqual('remove-last-level colour (level 1, disabled)', REMOVE_COLOUR_DISABLED, await style(ctx, REMOVE, 'color'));
  });

  test('multiclass buttons follow the prerequisites', async (ctx) => {
    await loadSeraphine(ctx);
    await openLevelUp(ctx);
    // Wizard INT is high, WIS/STR low: Cleric/Druid (Wisdom 13) disabled, Fighter (Str or Dex 13) and Rogue (Dex 13) per the sample scores.
    const states = await ctx.browser.evaluate((_: null) =>
      Object.fromEntries(Array.from(document.querySelectorAll('button[aria-label^="New multiclass "]')).map((b) => [b.getAttribute('aria-label'), (b as HTMLButtonElement).disabled])), null);
    expect(Object.keys(states)).toEqual(['New multiclass Cleric', 'New multiclass Druid', 'New multiclass Fighter', 'New multiclass Rogue']);
    expect(states['New multiclass Cleric']).toBe(true);
    expect(states['New multiclass Druid']).toBe(true);
    expect(states['New multiclass Fighter']).toBe(false);
    expectEqual('prerequisite line', 'Prerequisites: Strength 13 or Dexterity 13', (await textOf(ctx, '.lvl-mc-row[aria-label="Fighter multiclass"] .lvl-prereq')).trim());
  });

  // Multiclass and Remove last level: NEW MULTICLASS adds an empty slot, closes the flyout and opens Build > Class.
  test('NEW MULTICLASS: empty slot, flyout closes, Build > Class picker fills it; remove level drops the slot', async (ctx) => {
    await loadSeraphine(ctx);
    await openLevelUp(ctx);
    await clickCss(ctx, 'button[aria-label="New multiclass Fighter"]');
    await waitFor(ctx, (_: null) => !document.querySelector('.flyout'), null, 'flyout closed');
    await waitFor(ctx, (_: null) => location.hash.startsWith('#/builder/class'), null, 'route /builder/class');
    // Step 3: the Multiclass picker sits under the class group; classes failing ability prerequisites are hidden.
    await waitFor(ctx, (_: null) => document.querySelectorAll('.pick-main').length > 0, null, 'picker rows');
    const rows = await rowsText(ctx, 'section[aria-label^="Multiclass"] .pick-main');
    expect(rows.some((r) => r.startsWith('Fighter'))).toBe(true);
    expect(rows.some((r) => r.startsWith('Cleric'))).toBe(false);
    expect(rows.some((r) => r.startsWith('Druid'))).toBe(false);
    // Step 5: until a class is picked the level shows as EMPTY (n) with a hint to use the Multiclass picker.
    await openLevelUp(ctx);
    const empty = (await textOf(ctx, FLYOUT)).replace(/\n+/g, ' / ');
    expect(empty).toContain('YOUR CHARACTER IS LEVEL 3');
    expect(empty).toContain('EMPTY (1)');
    expect(empty).toContain('Multiclass (Level N) selection picker');
    // Remove level: the confirmation names the empty multiclass slot and the slot goes away.
    await clickCss(ctx, REMOVE);
    expectEqual('empty slot remove confirm', 'Remove the most recent level (the empty multiclass slot)? Its selections will be dropped.',
      await ctx.browser.evaluate((_: null) => ((Array.from(document.querySelectorAll('[role=dialog]')).pop() as HTMLElement | undefined)?.innerText ?? '').split('\n').find((l) => l.startsWith('Remove the most')) ?? '', null));
    await clickByText(ctx, '[role=dialog] button', 'Remove level');
    await waitFor(ctx, (_: null) => !/EMPTY/i.test(document.querySelector('.flyout')?.textContent ?? ''), null, 'empty slot removed');
    expect(await textOf(ctx, FLYOUT)).toContain('YOUR CHARACTER IS LEVEL 2');
    // Again, then pick Fighter from the picker: the slot fills and level-1 pickers are grouped under it.
    await clickCss(ctx, 'button[aria-label="New multiclass Fighter"]');
    await waitFor(ctx, (_: null) => location.hash.startsWith('#/builder/class') && !document.querySelector('.flyout'), null, 'back on Build > Class');
    await waitFor(ctx, (_: null) => Array.from(document.querySelectorAll('.pick-main')).some((b) => (b.textContent ?? '').startsWith('Fighter')), null, 'Fighter row');
    await dblClickByText(ctx, '.pick-main', 'Fighter');
    await waitFor(ctx, (_: null) => /FIGHTER/i.test(document.querySelector('.aur-profile')?.textContent ?? ''), null, 'Fighter in profile');
    expect(await profileLine(ctx)).toContain('Level 3 Elf Wizard');
    expect(await profileLine(ctx)).toContain('Fighter');
    // The filled slot now names the class in the remove-level confirmation.
    await openLevelUp(ctx);
    await clickCss(ctx, REMOVE);
    expectEqual('multiclass remove confirm', 'Remove the most recent level (Fighter)? Its selections will be dropped.',
      await ctx.browser.evaluate((_: null) => ((Array.from(document.querySelectorAll('[role=dialog]')).pop() as HTMLElement | undefined)?.innerText ?? '').split('\n').find((l) => l.startsWith('Remove the most')) ?? '', null));
    await clickByText(ctx, '[role=dialog] button', 'Remove level');
    await waitFor(ctx, (_: null) => !/FIGHTER/i.test(document.querySelector('.aur-profile')?.textContent ?? ''), null, 'Fighter slot removed');
    expect(await profileLine(ctx)).toContain('Level 2 Elf Wizard');
  });

  test('hit points: with Average hit points off every class level has an HP roll box ', async (ctx) => {
    const { browser } = ctx;
    await loadSeraphine(ctx);
    await browser.keyboard.press('F8');
    await clickByText(ctx, '.flyout label', 'Average hit points');
    await clickByText(ctx, '.flyout button', 'OK');
    await openLevelUp(ctx);
    await waitFor(ctx, (_: null) => document.querySelectorAll('.hp-rolls').length > 0, null, 'HP roll boxes');
    expect(await browser.evaluate((_: null) => document.querySelector('.hp-rolls')?.getAttribute('aria-label'), null)).toBe('Hit point rolls (d6)');
    // Wizard d6: level 1 is fixed at the die maximum; level 2 has an adjustable roll (1..6).
    expectEqual('level 1 HP (fixed at max)', 'Lv 1 6 (max)', (await textOf(ctx, '.hp-rolls .lvl-roll')).replace(/\s+/g, ' ').trim());
    const roll = 'output[aria-label="Wizard level 2 HP roll value"]';
    const before = Number(await rawTextOf(ctx, roll));
    await clickCss(ctx, 'button[aria-label="Increase Wizard level 2 HP roll"]');
    await waitFor(ctx, (a: { css: string; want: number }) => Number(document.querySelector(a.css)?.textContent) === a.want, { css: roll, want: before + 1 }, 'roll up by one');
    await clickCss(ctx, 'button[aria-label="Decrease Wizard level 2 HP roll"]');
    await waitFor(ctx, (a: { css: string; want: number }) => Number(document.querySelector(a.css)?.textContent) === a.want, { css: roll, want: before }, 'roll back');
  });
});
