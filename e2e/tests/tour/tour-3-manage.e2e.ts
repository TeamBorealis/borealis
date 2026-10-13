import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome } from '../../support/index.js';
import { clickByText, clickCss, clickMainTab, clickNth, clickSubTab, waitStatus } from '../../support/flows-a.js';
import { pressKey } from '../../support/flows-b.js';
import { caption, finishTour, pace, soft, startTour, tourClickLoc, tourDblClick, tourType } from '../../support/tour.js';

// Tour 3: Manage, compendium, settings, save, sources menu. Demo only.
const ROW = '[aria-label="Sources"] [role=row][aria-selected]';
describe('tour manage', { tags: ['tour'], requires: ['browser'] }, () => {
  test('tour-3-manage', { video: 'on', timeout: 300_000 }, async (raw) => {
    test.skip(!process.env.E2E_TOUR, 'tour only');
    const ctx = startTour(raw, 'tour-3-manage', 'Manage, compendium, settings, save and sources');
    const c = ctx as never;
    const { screen, browser } = ctx;

    await bootToHome(ctx);
    await caption(ctx, 'Open Seraphine Vale');
    await tourDblClick(ctx, '.v4-tile-open[aria-label="Seraphine Vale, Level 2 Elf Wizard"]');
    await waitStatus(c, (s) => s.startsWith('Loaded'));
    await pace(c, 1500);

    await caption(ctx, 'Manage: character details and experience');
    await soft(ctx, 'manage', async () => {
      await clickMainTab(c, 'Manage');
      await pace(c, 1800);
      await clickCss(c, 'button.mg-xp-btn');
      await pace(c, 600);
      await clickCss(c, 'button.mg-xp-btn');
      await pace(c, 1500);
    });
    await caption(ctx, 'Notes tab: type a note');
    await soft(ctx, 'notes', async () => {
      await clickSubTab(c, 'Notes');
      await pace(c, 1000);
      await tourType(ctx, '.mg-two textarea', 'Met the old sage at the crossroads. Owes me a favour.');
      await clickCss(c, '.mg-page h2, .mg-page');
      await pace(c, 1500);
    });
    await caption(ctx, 'Portrait: open the gallery, add an image, select it');
    await soft(ctx, 'portrait', async () => {
      await clickCss(c, '.aur-portrait-btn');
      await expect(screen.getByRole('dialog', 'Portraits gallery')).toBeVisible();
      await pace(c, 1500);
      await browser.locator('input[type=file][aria-label="Add a portrait image"]').setInputFiles('fixtures/portrait.png');
      await browser.locator('.pg-tile[aria-label="Portrait 1"]').waitFor({ state: 'visible' });
      await pace(c, 1500);
      await tourClickLoc(ctx, screen.getByRole('button', 'Select portrait'));
      await pace(c, 2000);
    });

    await caption(ctx, 'Compendium: Alt+Q, type a search, Enter');
    await soft(ctx, 'compendium', async () => {
      await pressKey(c, 'Alt+q');
      await pace(c, 800);
      const input = browser.locator('input[aria-label="Compendium search"]');
      for (const t of ['f', 'fi', 'fir', 'fire', 'fireb', 'firebal', 'fireball']) { await input.fill(t); await pace(c, 130); }
      await pressKey(c, 'Enter');
      await expect(screen.getByRole('dialog', 'Compendium')).toBeVisible();
      await pace(c, 1800);
      await clickCss(c, '.v4-el button[aria-selected]');
      await pace(c, 2500);
      await pressKey(c, 'Escape');
      await pace(c, 1000);
    });

    await caption(ctx, 'Settings: View > Application settings');
    await soft(ctx, 'settings', async () => {
      await tourClickLoc(ctx, screen.getByRole('menuitem', 'View'));
      await pace(c, 800);
      await tourClickLoc(ctx, screen.getByRole('menuitem', /^Application settings/i));
      await expect(screen.getByRole('dialog', 'Application Settings')).toBeVisible();
      await pace(c, 1500);
      const tabs = await browser.evaluate((_: null) => document.querySelectorAll('.aur-set-tabs [role=tab]').length, null);
      for (let i = 1; i < tabs; i++) { await clickNth(c, '.aur-set-tabs [role=tab]', i); await pace(c, 1100); }
      await caption(ctx, 'Escape closes Settings without saving');
      await pressKey(c, 'Escape');
      await pace(c, 1200);
    });

    await caption(ctx, 'Ctrl+S opens the Save flyout');
    await soft(ctx, 'save', async () => {
      await pressKey(c, 'Control+s');
      await expect(screen.getByRole('dialog', 'Save character')).toBeVisible();
      await pace(c, 2000);
      await tourType(ctx, '.flyout input[list]', 'Party');
      await pace(c, 800);
      await tourClickLoc(ctx, screen.getByRole('button', 'Save character'));
      await expect(screen.getByRole('dialog', 'Save character')).toBeHidden();
    });
    await caption(ctx, 'The status bar confirms: SAVED');
    await soft(ctx, 'saved status', async () => { await waitStatus(c, (s) => /SAVED/i.test(s)); await pace(c, 2500); });

    await caption(ctx, 'Sources: select a range, right-click for the menu');
    await soft(ctx, 'sources', async () => {
      await clickMainTab(c, 'Start');
      await pace(c, 800);
      await clickByText(c, '.tabs.sub a.tab', 'Sources');
      await expect(screen.getByRole('grid', 'Sources')).toBeVisible();
      await pace(c, 1500);
      const row = (i: number) => (browser.locator(ROW) as any).nth(i);
      await tourClickLoc(ctx, row(0));
      await pace(c, 600);
      await tourClickLoc(ctx, row(1), { modifiers: ['Shift'] });
      await pace(c, 1000);
      await tourClickLoc(ctx, row(1), { secondary: true });
      await expect(screen.getByRole('menu', 'Source')).toBeVisible();
      await pace(c, 2500);
      await caption(ctx, 'Toggle all selected sources');
      await tourClickLoc(ctx, screen.getByRole('menuitem', 'Toggle all selected sources'));
      await pace(c, 2500);
    });
    finishTour(ctx);
  });
});
