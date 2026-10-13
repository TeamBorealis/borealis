import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { expectEqual, bootToHome } from '../support/index.js';
import {
  clickByText, clickCss, clickSubTab, dblClickByText, dblClickCss, rawTextOf, textOf, waitFor, waitStatus,
} from '../support/flows-a.js';

// Create a new character: edition first, point buy cannot overspend, per-ability picker. Deterministic: no model.
const NEW_BTN = 'button.aur-newchar';
const CREATE = 'Create character';

async function openNewFlyout(ctx: Parameters<typeof bootToHome>[0] & { browser: any; screen: any }) {
  await bootToHome(ctx);
  await clickCss(ctx as never, NEW_BTN);
  await expect(ctx.screen.getByRole('dialog', 'New character')).toBeVisible();
}

describe('w1 create', { tags: ['w1'] }, () => {
  test('flyout order: edition, name, level, method, options; defaults', async (ctx) => {
    const { screen, browser } = ctx;
    await openNewFlyout(ctx);
    // The edition choice is part of the creation flyout; 2024 is the default.
    await expect(screen.getByRole('button', CREATE)).toBeVisible();
    await expect(screen.getByRole('button', 'Cancel')).toBeVisible();
    const labels = await browser.evaluate((_: null) => Array.from(document.querySelectorAll('.flyout label, .flyout h2')).map((e) => (e.textContent ?? '').trim()).filter(Boolean), null);
    expect(labels).toEqual(expect.arrayContaining(['Name', 'Starting level', 'Gender', 'Abilities generation option', 'Character options', 'Rules edition']));
    // Order: details block (name, level + gender, abilities option) first, character options after.
    expect(labels.indexOf('Name')).toBeLessThan(labels.indexOf('Starting level'));
    expect(labels.indexOf('Starting level')).toBeLessThan(labels.indexOf('Abilities generation option'));
    expect(labels.indexOf('Abilities generation option')).toBeLessThan(labels.indexOf('Rules edition'));
    expect(await browser.evaluate((_: null) => (document.querySelector('#v4-edition') as HTMLSelectElement).value, null)).toBe('dnd2024');
    expect(await browser.evaluate((_: null) => (document.querySelector('#v4-method') as HTMLSelectElement).value, null)).toBe('roll_4d6'); // default generation method
    expect(await browser.evaluate((_: null) => Array.from((document.querySelector('#v4-edition') as HTMLSelectElement).options).map((o) => o.textContent), null))
      .toEqual(['2024 - Revised rules (SRD 5.2)', '2014 - Classic rules (SRD 5.1)']);
    // Method list values.
    await screen.getByRole('button', 'Cancel').tap();
    await expect(screen.getByRole('dialog', 'New character')).toBeHidden();
  });

  test('standard array tiles show 15/14/13/12/10/8', async (ctx) => {
    const { browser } = ctx;
    await openNewFlyout(ctx);
    await browser.locator('#v4-method').selectOption({ value: 'standard_array' });
    await waitFor(ctx, (_: null) => document.querySelectorAll('.v4-tile-abil:not([disabled])').length === 6, null, 'six enabled tiles');
    const values = await browser.evaluate((_: null) => Array.from(document.querySelectorAll('.v4-tile-abil span')).map((e) => e.textContent), null);
    expect(values).toEqual(['15', '14', '13', '12', '10', '8']);
    // Swap through the tiles (press a tile, press another): no value ends up duplicated.
    await clickCss(ctx, '.v4-tile-abil');
    await clickByText(ctx, '.v4-tile-abil', 'CHA');
    const swapped = await browser.evaluate((_: null) => Array.from(document.querySelectorAll('.v4-tile-abil span')).map((e) => e.textContent), null);
    expect(swapped).toEqual(['8', '14', '13', '12', '10', '15']);
  });

  test('2014 create: status, then Species/Race -> Class -> Background pick order', async (ctx) => {
    const { browser } = ctx;
    await openNewFlyout(ctx);
    await browser.locator('#v4-name').fill('Test Hero');
    await browser.locator('#v4-edition').selectOption({ value: 'dnd2014' });
    await clickByText(ctx, '.flyout button', CREATE);
    await waitFor(ctx, (_: null) => location.hash === '#/builder/race', null, 'route /builder/race');
    // Build opens on the species/race picker with the engine prompt.
    const s0 = await waitStatus(ctx, (s) => s.startsWith('Choose one from'));
    expectEqual('first prompt (2014 terminology)', "Choose one from the 'Race' selection option.", s0);
    expect(await rawTextOf(ctx, '.aur-profile .name, .aur-profile')).toContain('Test Hero');
    expect(await textOf(ctx, 'main, .aur-content')).toContain('3 choices remaining');
    // Tabs in order.
    const tabs = await browser.evaluate((_: null) => Array.from(document.querySelectorAll('[role=tab]')).map((t) => (t.textContent ?? '').trim()), null);
    expect(tabs).toEqual(['Race', 'Class', 'Background', 'Ability Scores', 'Languages', 'Proficiencies', 'Feats']);
    // 7b/7c: double-click a race row; the next pending prompt is the class, then background.
    await dblClickByText(ctx, 'button.pick-main', 'Human');
    expectEqual('after race', "Choose one from the 'Class' selection option.", await waitStatus(ctx, (s) => s.includes("'Class'")));
    await clickSubTab(ctx, 'Class');
    await dblClickByText(ctx, 'button.pick-main', 'Wizard');
    expectEqual('after class', "Choose one from the 'Background' selection option.", await waitStatus(ctx, (s) => s.includes("'Background'")));
    await clickSubTab(ctx, 'Background');
    await dblClickByText(ctx, 'button.pick-main', 'Acolyte');
    expectEqual('after background', "Choose one from the 'Extra language' selection option.", await waitStatus(ctx, (s) => s.includes("'Extra language'")));
  });

  test('2024 default create: Species terminology and picker order', async (ctx) => {
    const { browser } = ctx;
    await openNewFlyout(ctx);
    await browser.locator('#v4-name').fill('Nova Test');
    await clickByText(ctx, '.flyout button', CREATE);
    await waitFor(ctx, (_: null) => location.hash === '#/builder/race', null, 'route /builder/race');
    // New character -> CREATE CHARACTER goes to /builder/race, status "...'Species' selection option."
    expectEqual('first prompt (2024 terminology)', "Choose one from the 'Species' selection option.", await waitStatus(ctx, (s) => s.startsWith('Choose one from')));
    const tabs = await browser.evaluate((_: null) => Array.from(document.querySelectorAll('[role=tab]')).map((t) => (t.textContent ?? '').trim()), null);
    expectEqual('first builder tab', 'Species', tabs[0] ?? '');
    expect(tabs.slice(1)).toEqual(['Class', 'Background', 'Ability Scores', 'Languages', 'Proficiencies', 'Feats']);
  });

  test('point buy never overspends the 27-point budget', async (ctx) => {
    const { browser } = ctx;
    await openNewFlyout(ctx);
    await browser.locator('#v4-name').fill('Budget Test');
    await browser.locator('#v4-method').selectOption({ value: 'point_buy' });
    await clickByText(ctx, '.flyout button', CREATE);
    await waitFor(ctx, (_: null) => location.hash === '#/builder/race', null, 'route /builder/race');
    await clickSubTab(ctx, 'Ability Scores');
    expectEqual('initial budget line', 'Ability points: 0 / 27', await textOf(ctx, '[aria-label="Point buy budget"]'));
    const inc = (a: string) => `button[aria-label="Increase ${a}"]`;
    // 8 -> 15 costs 9 points each: three abilities use the full 27.
    for (const a of ['Strength', 'Dexterity', 'Constitution']) {
      for (let i = 0; i < 7; i++) await clickCss(ctx, inc(a));
    }
    await waitFor(ctx, (_: null) => document.querySelector('[aria-label="Point buy budget"]')?.textContent?.includes('27 / 27') ?? false, null, 'budget spent');
    expectEqual('spent budget line', 'Ability points: 27 / 27', await textOf(ctx, '[aria-label="Point buy budget"]'));
    // Nothing left to spend, so every further increase is disabled; no way to overspend.
    for (const a of ['Intelligence', 'Wisdom', 'Charisma']) {
      expect(await browser.evaluate((css: string) => (document.querySelector(css) as HTMLButtonElement).disabled, inc(a))).toBe(true);
    }
    expect(await browser.evaluate((_: null) => document.querySelector('[aria-label="Point buy budget"]')?.classList.contains('over') ?? false, null)).toBe(false);
  });

  test('standard array: rearranged tiles carry over; per-ability picker swaps instead of duplicating', async (ctx) => {
    const { browser } = ctx;
    await openNewFlyout(ctx);
    await browser.locator('#v4-name').fill('Array Test');
    await browser.locator('#v4-method').selectOption({ value: 'standard_array' });
    await waitFor(ctx, (_: null) => document.querySelectorAll('.v4-tile-abil:not([disabled])').length === 6, null, 'six enabled tiles');
    // Swap the STR and CHA tiles (press one, press the other).
    await clickByText(ctx, '.v4-tile-abil', 'STR');
    await clickByText(ctx, '.v4-tile-abil', 'CHA');
    await clickByText(ctx, '.flyout button', CREATE);
    await waitFor(ctx, (_: null) => location.hash === '#/builder/race', null, 'route /builder/race');
    await clickSubTab(ctx, 'Ability Scores');
    const bases = () => browser.evaluate((_: null) => Array.from(document.querySelectorAll('.abil-base')).map((e) => e.textContent), null);
    await waitFor(ctx, (_: null) => document.querySelector('.abil-base')?.textContent === '8', null, 'arrangement applied');
    expect(await bases()).toEqual(['8', '14', '13', '12', '10', '15']);
    // A per-ability picker, one per ability (not one value dumped on STR).
    expect(await browser.evaluate((_: null) => document.querySelectorAll('select[aria-label$="array value"]').length, null)).toBe(6);
    // Pick 15 for Dexterity: 15 is in use on Charisma, so the two swap and all six values stay unique.
    await browser.locator('select[aria-label="Dexterity array value"]').selectOption({ value: '15' });
    await waitFor(ctx, (_: null) => document.querySelectorAll('.abil-base')[1]?.textContent === '15', null, 'DEX base 15');
    expect(await bases()).toEqual(['8', '15', '13', '12', '10', '14']);
    expect(await textOf(ctx, '.abil-note')).toContain('Unassigned values: none');
  });

  // Regression: the flyout previews 15/14/13/12/10/8 on STR..CHA, but leaving the arrangement untouched creates a character with every score at 8.
  test('standard array left in the default arrangement creates STR 15 ... CHA 8', async (ctx) => {
    const { browser } = ctx;
    await openNewFlyout(ctx);
    await browser.locator('#v4-name').fill('Default Array');
    await browser.locator('#v4-method').selectOption({ value: 'standard_array' });
    await clickByText(ctx, '.flyout button', CREATE);
    await waitFor(ctx, (_: null) => location.hash === '#/builder/race', null, 'route /builder/race');
    await clickSubTab(ctx, 'Ability Scores');
    const bases = await browser.evaluate((_: null) => Array.from(document.querySelectorAll('.abil-base')).map((e) => e.textContent), null);
    expect(bases).toEqual(['15', '14', '13', '12', '10', '8']);
  });
});

