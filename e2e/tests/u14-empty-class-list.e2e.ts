import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, realClick } from '../support/index.js';
import { clickByText, clickSubTab, textOf, waitFor } from '../support/flows-a.js';

// GitHub #10 (decision U14). The mock host's `?mockClassless=dnd2024` takes every 2024 class away, like a user whose 2024
// books are all switched off. The New Character dialog warns; the 2024 character's empty Class list names the edition with
// a count (not "check your source restrictions"); "Start over as 2014" confirms, then opens a NEW 2014 character whose
// Class list has rows, and the 2024 character stays in the collection. Real pointer clicks; deterministic, no model.
const NOTE = '[role=note][aria-label="Why this list is empty"]';

describe('#10 empty Class list', { tags: ['u14'] }, () => {
  test('a 2024 character without 2024 classes learns why and starts over as 2014', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx, '/?mockClassless=dnd2024#/characters');
    await realClick(ctx, browser.locator('button.aur-newchar'), 'button.aur-newchar');
    await expect(screen.getByRole('dialog', 'New character')).toBeVisible();
    await browser.locator('#v4-name').fill('Ayla Thornwick');
    // the edition defaults to 2024 (Settings), the one without classes here
    await waitFor(ctx, (_: null) => (document.querySelector('.v4-warn')?.textContent ?? '').startsWith('No 2024 classes are available'), null, 'no-classes warning');
    await clickByText(ctx, '.flyout button', 'Create character');
    await waitFor(ctx, (_: null) => location.hash === '#/builder/race', null, 'route /builder/race');

    await clickSubTab(ctx, 'Class');
    await waitFor(ctx, (css: string) => !!document.querySelector(css), NOTE, 'empty-list note');
    const note = await textOf(ctx, NOTE);
    expect(note.startsWith('This character uses the 2024 rules.')).toBe(true);
    expect(/classes from 2014 books need a 2014 character/.test(note)).toBe(true);
    expect(/source restrictions/i.test(note)).toBe(false);

    await realClick(ctx, browser.locator(`${NOTE} button`), `${NOTE} button`);
    await expect(screen.getByRole('dialog', 'Start over as 2014?')).toBeVisible();
    await realClick(ctx, browser.locator('[role=dialog] .btn-primary'), '[role=dialog] .btn-primary');
    await waitFor(ctx, (css: string) => !document.querySelector(css) && document.querySelectorAll('.pick-list li[data-row]').length > 0, NOTE, '2014 class rows');

    await screen.getByRole('menuitem', 'File').tap();
    await screen.getByRole('menuitem', 'Load Character').tap();
    await waitFor(ctx, (_: null) => document.querySelectorAll('.v4-tile-open[aria-label^="Ayla Thornwick"]').length === 2, null, 'both characters in the collection');
  });
});
