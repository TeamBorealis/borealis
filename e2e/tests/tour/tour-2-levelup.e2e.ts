import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome } from '../../support/index.js';
import { pressKey } from '../../support/flows-b.js';
import { clickByText, clickCss, clickMainTab, clickSubTab, dblClickByText, openLevelUp, waitFor, waitStatus } from '../../support/flows-a.js';
import { caption, finishTour, pace, soft, startTour, tourDblClick, tourPoint } from '../../support/tour.js';

// Tour 2: open a seeded character, level up, new multiclass, remove level. Demo only.
const REMOVE = 'button.lvl-link.remove';
describe('tour levelup', { tags: ['tour'], requires: ['browser'] }, () => {
  test('tour-2-levelup', { video: 'on', timeout: 300_000 }, async (raw) => {
    test.skip(!process.env.E2E_TOUR, 'tour only');
    const ctx = startTour(raw, 'tour-2-levelup', 'Level up, multiclass and remove a level');
    const c = ctx as never;
    const { screen } = ctx;

    await bootToHome(ctx);
    await caption(ctx, 'Open a saved character: double-click Seraphine Vale');
    await pace(c, 1200);
    await tourDblClick(ctx, '.v4-tile-open[aria-label="Seraphine Vale, Level 2 Elf Wizard"]');
    await waitStatus(c, (s) => s.startsWith('Loaded'));
    await pace(c, 2000);

    await caption(ctx, 'Level Up: opens the level flyout');
    await openLevelUp(c);
    await pace(c, 3200);
    await caption(ctx, 'Level up the Wizard: hit points and new choices appear');
    await clickCss(c, 'button[aria-label="Level up Wizard"]');
    await waitFor(c, (_: null) => /LEVEL 3 ELF WIZARD/i.test(document.querySelector('.aur-profile')?.textContent ?? ''), null, 'level 3');
    await pace(c, 2500);
    await caption(ctx, 'The profile header shows the new level and hit points');
    await soft(ctx, 'hp roll', async () => {
      await tourPoint(ctx, '.aur-profile');
      await pace(c, 2500);
    });

    await caption(ctx, 'New multiclass: Fighter adds an empty slot');
    await soft(ctx, 'new multiclass', async () => {
      await clickCss(c, 'button[aria-label="New multiclass Fighter"]');
      await pace(c, 2200);
    });
    await soft(ctx, 'multiclass flyout state', async () => {
      const open = await ctx.browser.evaluate((_: null) => !!document.querySelector('.flyout'), null);
      const route = await ctx.browser.evaluate((_: null) => location.hash, null);
      if (open) throw new Error(`flyout still open after NEW MULTICLASS, route ${route}`);
    });
    await caption(ctx, 'Build > Class: choose the class in the Multiclass picker');
    await soft(ctx, 'class picker', async () => {
      await waitFor(c, (_: null) => location.hash.startsWith('#/builder/class') && !document.querySelector('.flyout'), null, 'class route');
      await pace(c, 1500);
      await dblClickByText(c, '.pick-main', 'Fighter');
      await pace(c, 2200);
    });

    await caption(ctx, 'Remove the most recent level: the app asks first');
    await soft(ctx, 'remove level', async () => {
      if (!(await ctx.browser.evaluate((_: null) => !!document.querySelector('.flyout'), null))) await openLevelUp(c);
      await pace(c, 1200);
      await clickCss(c, REMOVE);
      await expect(screen.getByRole('dialog', 'Remove last level')).toBeVisible();
      await pace(c, 2500);
      await clickByText(c, '[role=dialog] button', 'Remove level');
      await pace(c, 2200);
    });
    await caption(ctx, 'Fighter slot gone: level 3 Wizard again');
    await soft(ctx, 'close', async () => { await pressKey(c, 'Escape'); await pace(c, 800); await clickMainTab(c, 'Build'); await pace(c, 800); await clickSubTab(c, 'Class'); await pace(c, 2000); });
    finishTour(ctx);
  });
});
