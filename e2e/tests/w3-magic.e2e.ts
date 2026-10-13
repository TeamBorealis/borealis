import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { expectEqual } from '../support/index.js';
import {
  clickByText, clickCellInRow, clickCss, clickMainTab, clickSubTab, count, dblClickByText, isDisabled, loadSeraphine, rawTextOf, rowsText, textOf, waitFor, waitStatus,
} from '../support/flows-a.js';

// Choose spells and prepare them: replace a spell, refill reopened slots, class-tab Prepare, full-row selection. Seeded: Seraphine Vale, Wizard 2. Deterministic: no model.
const SLOT_ROWS = "table[aria-label='Selection slots'] tbody tr";
const OPTION_GRID = 'table.sp-grid[data-slot-id]';
const CLASS_ROWS = 'table.sp-grid tbody tr';
const PREPARE = '.sp-prep-bar button:first-of-type';

async function openMagic(ctx: Parameters<typeof loadSeraphine>[0]) {
  await loadSeraphine(ctx);
  await clickMainTab(ctx, 'Magic');
  await waitFor(ctx, (_: null) => location.hash.startsWith('#/spells'), null, 'route /spells');
  await waitFor(ctx, (_: null) => document.querySelectorAll("table[aria-label='Selection slots'] tbody tr").length > 0, null, 'slot rows');
}

describe('w3 magic', { tags: ['w3'] }, () => {
  test('Magic page: tabs, stats band and slot table columns', async (ctx) => {
    const { browser } = ctx;
    await openMagic(ctx);
    const tabs = await browser.evaluate((_: null) => Array.from(document.querySelectorAll('[role=tab]')).map((t) => (t.textContent ?? '').trim()), null);
    expect(tabs).toEqual(['Spellcasting', 'WIZARD']);
    const text = (await textOf(ctx, '.aur-content')).replace(/\s+/g, ' ');
    expect(text).toContain('CANTRIPS:3/3');
    expect(text).toContain('PREPARED:4/5');
    // Slot table: the five columns in order.
    const head = await browser.evaluate((_: null) => Array.from(document.querySelectorAll("table[aria-label='Selection slots'] thead th")).map((t) => (t.textContent ?? '').trim().toUpperCase()).filter(Boolean), null);
    expect(head).toEqual(['NAME', 'SELECTED SPELL', 'GAINED ON LEVEL', 'REPLACED', 'SPELLCASTING SOURCE']);
    expect((await rowsText(ctx, SLOT_ROWS))[0]).toBe('Wizard cantrip (1 of 3) Fire Bolt 1 Wizard');
  });

  test('Replace spell then refill the reopened slot', async (ctx) => {
    await openMagic(ctx);
    // Cantrips cannot be replaced in 2014 rules: no Replace button for a highlighted cantrip.
    await clickCellInRow(ctx, SLOT_ROWS, 'Wizard cantrip (1 of 3)', 'td:nth-child(2)');
    expect(await count(ctx, '.sp-replace')).toBe(0);
    // Select the filled Magic Missile slot, then press Replace spell with the real pointer.
    await clickCellInRow(ctx, SLOT_ROWS, 'Magic Missile', 'td:nth-child(3)');
    await waitFor(ctx, (_: null) => !!document.querySelector('.sp-replace'), null, 'Replace spell button');
    expect((await rawTextOf(ctx, '.sp-replace')).toUpperCase()).toBe('REPLACE SPELL');
    await clickCss(ctx, '.sp-replace');
    // Status + row text after the replace.
    expectEqual('replace status', "Wizard: Magic Missile is replaced. Choose one from the 'Spellbook spell' selection option.",
      await waitStatus(ctx, (s) => s.includes('is replaced')));
    expect(await rowsText(ctx, SLOT_ROWS)).toContain('Spellbook spell (1 of 8) not selected 2 Magic Missile Wizard');
    // The slot's options show in the catalogue; the replaced spell is offered again (3 options in the mock).
    await waitFor(ctx, (_: null) => document.querySelectorAll('table.sp-grid[data-slot-id] tbody tr').length === 3, null, 'three slot options');
    expect(await rowsText(ctx, `${OPTION_GRID} tbody tr`)).toEqual(expect.arrayContaining([expect.stringContaining('Magic Missile')]));
    // Refill by double-clicking a catalogue row.
    await dblClickByText(ctx, `${OPTION_GRID} .ap-name`, 'Burning Hands');
    expectEqual('refill status', 'All required selections are made.', await waitStatus(ctx, (s) => s.startsWith('All required')));
    expect(await rowsText(ctx, SLOT_ROWS)).toContain('Spellbook spell (1 of 8) Burning Hands 2 Magic Missile Wizard');
    // Replace history and the "Replaced" column persist; the slot count did not change.
    expect((await rowsText(ctx, SLOT_ROWS)).length).toBe(11);
  });

  test('class tab: hint, Prepare/Unprepare by button, double-click and status text', async (ctx) => {
    await openMagic(ctx);
    await clickSubTab(ctx, 'WIZARD');
    await waitFor(ctx, (_: null) => !!document.querySelector('.sp-prep-bar'), null, 'prepare bar');
    // Hint + button labels in order.
    expectEqual('class-tab hint', 'Double click a spell to prepare or unprepare it.', await rawTextOf(ctx, '.sp-prep-bar .sp-hint'));
    const labels = await ctx.browser.evaluate((_: null) => Array.from(document.querySelectorAll('.sp-prep-bar button')).map((b) => (b.textContent ?? '').trim().toUpperCase()), null);
    expect(labels).toEqual(['PREPARE', 'REFRESH']);
    // PREPARE stays disabled until a row is highlighted.
    expect(await isDisabled(ctx, PREPARE)).toBe(true);
    // No learn expander and no duplicate Known list on the class tab.
    expect((await textOf(ctx, '.aur-content')).toUpperCase()).not.toContain('KNOWN SPELLS');
    // Highlight Detect Magic by its name, PREPARE with the real pointer.
    await clickByText(ctx, `${CLASS_ROWS} .ap-name`, 'Detect Magic');
    await waitFor(ctx, (_: null) => !(document.querySelector('.sp-prep-bar button') as HTMLButtonElement).disabled, null, 'PREPARE enabled');
    await clickCss(ctx, PREPARE);
    expectEqual('prepare status', 'Wizard: Detect Magic is prepared.', await waitStatus(ctx, (s) => s.includes('prepared')));
    await waitFor(ctx, (_: null) => /unprepare/i.test(document.querySelector('.sp-prep-bar button')?.textContent ?? ''), null, 'button flips to UNPREPARE');
    // Double-click toggles it back and reports it.
    await dblClickByText(ctx, `${CLASS_ROWS} .ap-name`, 'Detect Magic');
    expectEqual('unprepare status', 'Wizard: Detect Magic is unprepared.', await waitStatus(ctx, (s) => s.includes('unprepared')));
    // A cantrip never needs preparing: info box, button disabled.
    await clickByText(ctx, `${CLASS_ROWS} .ap-name`, 'Fire Bolt');
    await waitFor(ctx, (_: null) => !!document.querySelector('.sp-info'), null, 'info box');
    expectEqual('cantrip info', "Fire Bolt (cantrip) is always ready to cast and doesn't need to be prepared.", (await rawTextOf(ctx, '.sp-info')));
    expect(await isDisabled(ctx, PREPARE)).toBe(true);
  });

  test('class tab: a real click on "Level & school" selects the row and enables PREPARE', async (ctx) => {
    await openMagic(ctx);
    await clickSubTab(ctx, 'WIZARD');
    await waitFor(ctx, (_: null) => !!document.querySelector('.sp-prep-bar'), null, 'prepare bar');
    expect(await isDisabled(ctx, PREPARE)).toBe(true);
    expect(await count(ctx, 'table.sp-grid tr.sel')).toBe(0);
    await clickCellInRow(ctx, CLASS_ROWS, 'Detect Magic', 'td.sp-lvl');
    await waitFor(ctx, (_: null) => document.querySelectorAll('table.sp-grid tr.sel').length === 1, null, 'one selected row');
    expect((await rowsText(ctx, 'table.sp-grid tr.sel'))[0]).toContain('Detect Magic');
    expect(await isDisabled(ctx, PREPARE)).toBe(false);
    await clickCss(ctx, PREPARE);
    expectEqual('prepare status via cell selection', 'Wizard: Detect Magic is prepared.', await waitStatus(ctx, (s) => s.includes('prepared')));
  });

  test('slot options grid: a real click on a non-name cell highlights the row', async (ctx) => {
    await openMagic(ctx);
    await clickCellInRow(ctx, SLOT_ROWS, 'Magic Missile', 'td:nth-child(3)');
    await clickCss(ctx, '.sp-replace');
    await waitFor(ctx, (_: null) => document.querySelectorAll('table.sp-grid[data-slot-id] tbody tr').length > 0, null, 'slot options');
    await clickCellInRow(ctx, `${OPTION_GRID} tbody tr`, 'Charm Person', 'td.sp-lvl');
    await waitFor(ctx, (_: null) => document.querySelectorAll('table.sp-grid[data-slot-id] tr.sel').length === 1, null, 'highlighted option');
    expect((await rowsText(ctx, `${OPTION_GRID} tr.sel`))[0]).toContain('Charm Person');
  });
});
