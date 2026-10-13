import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome } from '../../support/index.js';
import {
  clickByText, clickCellInRow, clickCss, clickMainTab, clickSubTab, dblClickByText, waitFor, waitStatus,
} from '../../support/flows-a.js';
import { caption, finishTour, pace, soft, startTour, tourDblClick, tourType } from '../../support/tour.js';

// Tour 1: create a character and walk through Build, Magic, Equipment, Sheet. Demo only.
describe('tour create', { tags: ['tour'], requires: ['browser'] }, () => {
  test('tour-1-create', { video: 'on', timeout: 300_000 }, async (raw) => {
    test.skip(!process.env.E2E_TOUR, 'tour only');
    const ctx = startTour(raw, 'tour-1-create', 'Create a character: Build, Magic, Equipment, Sheet');
    const { screen, browser } = ctx;
    const c = ctx as never;
    const pick = (label: string, css: string, text: string) => soft(ctx, label, async () => { await dblClickByText(c, css, text); await pace(c, 1200); });

    await bootToHome(ctx);
    await caption(ctx, 'Start screen with the saved characters');
    await pace(c, 1500);

    await caption(ctx, 'New character: opens the creation flyout');
    await clickCss(c, 'button.aur-newchar');
    await expect(screen.getByRole('dialog', 'New character')).toBeVisible();
    await pace(c, 1500);
    await caption(ctx, 'Type a name; rules edition is 2024');
    await tourType(ctx, '#v4-name', 'Mirelle Dawnwhisper');
    await pace(c);
    await caption(ctx, 'Abilities generation option: Standard Array');
    await browser.locator('#v4-method').selectOption({ value: 'standard_array' });
    await waitFor(c, (_: null) => document.querySelectorAll('.v4-tile-abil:not([disabled])').length === 6, null, 'tiles');
    await pace(c, 1800);
    await caption(ctx, 'Create character');
    await clickByText(c, '.flyout button', 'Create character');
    await waitFor(c, (_: null) => location.hash === '#/builder/race', null, 'builder');
    await pace(c, 1500);

    await caption(ctx, 'Build > Species: double-click to pick');
    await pace(c, 800);
    await pick('species', 'button.pick-main', 'Elf');
    await caption(ctx, 'Species brings a lineage choice');
    await soft(ctx, 'lineage', async () => { await tourDblClick(c, 'button.pick-main', 0); await pace(c, 1500); });
    await caption(ctx, 'Build > Class: Wizard');
    await soft(ctx, 'class tab', () => clickSubTab(c, 'Class'));
    await pace(c);
    await pick('class', 'button.pick-main', 'Wizard');
    await caption(ctx, 'Build > Background');
    await soft(ctx, 'bg tab', () => clickSubTab(c, 'Background'));
    await pace(c);
    await pick('background', 'button.pick-main', 'Acolyte');
    await caption(ctx, 'Background ability increases: +2 and +1');
    await pick('bg increase +2', 'button.pick-main', '+2 Intelligence');
    await pick('bg increase +1', 'button.pick-main', '+1 Wisdom');
    await caption(ctx, 'Ability Scores: the standard array is assigned');
    await soft(ctx, 'abilities', async () => { await clickSubTab(c, 'Ability Scores'); await pace(c, 2200); });
    await caption(ctx, 'Languages');
    await soft(ctx, 'languages', async () => { await clickSubTab(c, 'Languages'); await pace(c, 1500); });
    await soft(ctx, 'pick language', async () => { await clickByText(c, 'button.pick-main', 'Elvish').catch(() => dblClickByText(c, 'button.pick-main', 'Dwarvish')); await pace(c); });
    await caption(ctx, 'Proficiencies');
    await soft(ctx, 'proficiencies', async () => { await clickSubTab(c, 'Proficiencies'); await pace(c, 1800); });
    await soft(ctx, 'pick prof', async () => { await dblClickByText(c, 'button.pick-main', 'Arcana'); await pace(c); });
    await soft(ctx, 'status', async () => { await waitStatus(c, () => true, 200); });
    await pace(c, 1200);

    await caption(ctx, 'Magic: spellcasting for the new Wizard');
    await soft(ctx, 'magic', async () => {
      await clickMainTab(c, 'Magic');
      await waitFor(c, (_: null) => location.hash.startsWith('#/spells'), null, 'spells');
      await pace(c, 2000);
    });
    const SLOTS = "table[aria-label='Selection slots'] tbody tr";
    const OPTION = 'table.sp-grid[data-slot-id] .ap-name';
    await caption(ctx, 'Select an open cantrip slot, then double-click a spell to fill it');
    await soft(ctx, 'pick cantrip', async () => {
      await waitFor(c, (_: null) => document.querySelectorAll("table[aria-label='Selection slots'] tbody tr").length > 0, null, 'slots');
      await clickCellInRow(c, SLOTS, 'Wizard cantrip (1 of 3)', 'td:nth-child(2)');
      await pace(c, 1500);
      await tourDblClick(c, OPTION, 0);
      await pace(c, 1800);
    });
    await caption(ctx, 'Same for a spellbook slot');
    await soft(ctx, 'pick spellbook spell', async () => {
      await clickCellInRow(c, SLOTS, 'Spellbook spell (1 of 6)', 'td:nth-child(2)');
      await pace(c, 1500);
      await tourDblClick(c, OPTION, 0);
      await pace(c, 1800);
    });
    await caption(ctx, 'Wizard tab: highlight a known spell and prepare it');
    await soft(ctx, 'prepare', async () => {
      await clickSubTab(c, 'WIZARD');
      await waitFor(c, (_: null) => !!document.querySelector('.sp-prep-bar'), null, 'prep bar');
      await pace(c, 1200);
      await clickCss(c, 'table.sp-grid tbody tr .ap-name');
      await pace(c, 800);
      await clickCss(c, '.sp-prep-bar button:first-of-type');
      await pace(c, 2000);
    });

    await caption(ctx, 'Equipment: browse the catalogue');
    await soft(ctx, 'equipment', async () => {
      await clickMainTab(c, 'Equipment');
      await waitFor(c, (_: null) => location.hash.startsWith('#/equipment') && document.querySelectorAll('.eq-grid tbody tr').length > 0, null, 'equipment');
      await pace(c, 1500);
      await clickByText(c, 'button', 'Weapons (');
      await pace(c, 1200);
    });
    await caption(ctx, 'Select an item and Add it to the inventory');
    await soft(ctx, 'add item', async () => {
      await clickCellInRow(c, '.eq-grid tbody tr', 'Dagger', 'td:nth-child(2)').catch(() => clickCss(c, '.eq-grid tbody tr td'));
      await pace(c, 800);
      await clickByText(c, 'button.eq-cmd-btn', 'Add');
      await pace(c, 1500);
    });
    await caption(ctx, 'Equip the new item');
    await soft(ctx, 'equip', async () => {
      await clickCss(c, 'button[aria-label^="Equip "]');
      await pace(c, 1500);
    });
    await caption(ctx, 'The app offers to add the weapon to attacks');
    await soft(ctx, 'attacks prompt', async () => {
      await clickByText(c, '[role=dialog] button', 'Add attack');
      await pace(c, 1500);
    });

    await caption(ctx, 'Character Sheet preview');
    await soft(ctx, 'sheet', async () => {
      await clickMainTab(c, 'Character Sheet');
      await pace(c, 3000);
    });
    await caption(ctx, 'End of tour 1');
    await pace(c, 1200);
    finishTour(ctx);
  });
});
