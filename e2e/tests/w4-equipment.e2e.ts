import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { expectEqual } from '../support/index.js';
import {
  boxOf, clickByText, clickCellInRow, clickCss, clickMainTab, loadSeraphine, rowsText, textOf, waitFor, waitStatus,
} from '../support/flows-a.js';

// Manage equipment: category list, armor grid, full-row select. Seeded: Seraphine Vale.
// Deterministic: no model.
const CAT_ROWS = '.eq-grid tbody tr';
const ADD = 'button.eq-cmd-btn';

async function openEquipment(ctx: Parameters<typeof loadSeraphine>[0]) {
  await loadSeraphine(ctx);
  await clickMainTab(ctx, 'Equipment');
  await waitFor(ctx, (_: null) => location.hash.startsWith('#/equipment') && document.querySelectorAll('.eq-grid tbody tr').length > 0, null, 'equipment catalogue');
}

async function openArmor(ctx: Parameters<typeof loadSeraphine>[0]) {
  await openEquipment(ctx);
  await clickByText(ctx, 'button', 'Armor (');
  await waitFor(ctx, (_: null) => Array.from(document.querySelectorAll('.eq-grid tbody tr')).some((r) => (r.textContent ?? '').includes('Chain Mail')), null, 'armor rows');
}

const addDisabled = (ctx: Parameters<typeof loadSeraphine>[0]) => ctx.browser.evaluate((_: null) =>
  (Array.from(document.querySelectorAll('button.eq-cmd-btn')).find((b) => (b.textContent ?? '').trim() === 'Add') as HTMLButtonElement).disabled, null);

describe('w4 equipment', { tags: ['w4'] }, () => {
  test('page layout: categories, coin purse, inventory columns', async (ctx) => {
    await openEquipment(ctx);
    const text = (await textOf(ctx, '.aur-content')).replace(/\s+/g, ' ');
    // Category list in order, with counts.
    const cats = ['Adventuring Gear (6)', 'Equipment Packs (2)', 'Tools (2)', 'Armor (5)', 'Magic Armor (0)', 'Weapons (8)', 'Magic Weapons (0)'];
    let at = -1;
    for (const c of cats) {
      const n = text.indexOf(c, at + 1);
      expect(n, `category "${c}" in order`).toBeGreaterThan(at);
      at = n;
    }
    expect(text).toContain('GOLD:150');
    expect(text).toContain('MANAGE COINS: 150 GP');
    expect(text).toContain('ATTUNED ITEMS:0');
    expect(text).toContain('NAME EQUIP LOCATION');
    // The Add button needs a selected row.
    expect(await addDisabled(ctx)).toBe(true);
  });

  test('Armor grid: column order and first row', async (ctx) => {
    const { browser } = ctx;
    await openArmor(ctx);
    const head = await browser.evaluate((_: null) => Array.from(document.querySelector('.eq-grid')!.querySelectorAll('thead th')).map((t) => (t.textContent ?? '').trim().toUpperCase()), null);
    expect(head).toEqual(['NAME', 'PRICE', 'WEIGHT', 'ARMOR CLASS', 'STRENGTH', 'STEALTH', 'GROUP', 'SOURCE']);
    expect((await rowsText(ctx, CAT_ROWS))[0]).toBe('Chain Mail 75 gp 55 lb. 16 13 Disadvantage Heavy Sample Reference 5.1');
  });

  test('a real click on a non-name cell selects the row and enables ADD', async (ctx) => {
    await openArmor(ctx);
    expect(await addDisabled(ctx)).toBe(true);
    // The grid is full-row: a click on the price cell selects the row.
    await clickCellInRow(ctx, CAT_ROWS, 'Chain Mail', 'td.k-price');
    await waitFor(ctx, (_: null) => document.querySelectorAll('.eq-grid tr.sel').length === 1, null, 'selected row');
    expect((await rowsText(ctx, '.eq-grid tr.sel'))[0]).toContain('Chain Mail');
    expect(await addDisabled(ctx)).toBe(false);
  });

  test('add armor, equip it, unequip it, remove it', async (ctx) => {
    await openArmor(ctx);
    await clickCellInRow(ctx, CAT_ROWS, 'Chain Mail', 'td.k-weight');
    await waitFor(ctx, (_: null) => document.querySelectorAll('.eq-grid tr.sel').length === 1, null, 'selected row');
    await clickByText(ctx, ADD, 'Add');
    expectEqual('add status', "You have added 1 x 'Chain Mail' to your inventory.", await waitStatus(ctx, (s) => s.startsWith('You have added')));
    await waitFor(ctx, (_: null) => !!document.querySelector('button[aria-label="Equip Chain Mail"]'), null, 'Chain Mail in inventory');
    expect(await rowsText(ctx, 'tr')).toEqual(expect.arrayContaining([expect.stringMatching(/^Chain Mail\s*$/)]));
    // Equip with the real pointer: the toggle flips to pressed and the location column reads "Armor".
    await clickCss(ctx, 'button[aria-label="Equip Chain Mail"]');
    await waitFor(ctx, (_: null) => document.querySelector('button.eq-eqbtn.on')?.getAttribute('aria-label') === 'Unequip Chain Mail', null, 'equipped');
    expect((await rowsText(ctx, 'tr')).some((t) => /^Chain Mail Armor$/.test(t))).toBe(true);
    // Unequip, then equip again so removing deletes an equipped stack's single unit.
    await clickCss(ctx, 'button[aria-label="Unequip Chain Mail"]');
    await waitFor(ctx, (_: null) => !!document.querySelector('button[aria-label="Equip Chain Mail"]'), null, 'unequipped');
    // Remove: select the inventory row, Delete removes one unit.
    await clickCss(ctx, 'button[aria-label="Select Chain Mail"]');
    await clickByText(ctx, 'button', 'Delete');
    expectEqual('remove status', "You have removed 1 x 'Chain Mail' from your inventory.", await waitStatus(ctx, (s) => s.startsWith('You have removed')));
    await waitFor(ctx, (_: null) => !document.querySelector('button[aria-label="Select Chain Mail"]'), null, 'gone from inventory');
  });

  test('add a pack from Equipment Packs; inventory keeps starting gear', async (ctx) => {
    await openEquipment(ctx);
    await clickByText(ctx, 'button', 'Equipment Packs (');
    await waitFor(ctx, (_: null) => Array.from(document.querySelectorAll('.eq-grid tbody tr')).some((r) => (r.textContent ?? '').includes('Pack')), null, 'pack rows');
    await clickCellInRow(ctx, CAT_ROWS, 'Pack', 'td:nth-child(2)');
    await waitFor(ctx, (_: null) => document.querySelectorAll('.eq-grid tr.sel').length === 1, null, 'selected pack');
    await clickByText(ctx, ADD, 'Add');
    const s = await waitStatus(ctx, (m) => m.startsWith('You have added'));
    expect(s).toMatch(/^You have added 1 x '.*Pack' to your inventory\.$/);
    // Starting gear stays: Spellbook, Dagger, Explorer's Pack.
    const inv = (await textOf(ctx, '.aur-content')).replace(/\s+/g, ' ');
    for (const n of ['Spellbook', 'Dagger', "Explorer's Pack"]) expect(inv).toContain(n);
  });

  test('inventory: a real click on the location cell selects the row (FullRow)', async (ctx) => {
    await openEquipment(ctx);
    // Inventory rows: Spellbook, Dagger, Explorer's Pack. "Dagger" only exists in the inventory (the catalogue shows Adventuring Gear).
    expect(await rowsText(ctx, '.eq-grid tr.sel')).toEqual([]);
    await clickCellInRow(ctx, CAT_ROWS, 'Dagger', 'td:last-child');
    await waitFor(ctx, (_: null) => document.querySelectorAll('.eq-grid tr.sel').length === 1, null, 'inventory row selected');
    expect((await rowsText(ctx, '.eq-grid tr.sel'))[0]).toContain('Dagger');
  });
});
