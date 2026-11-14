import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome } from '../support/index.js';
import { AxeReport, LANDMARK_RULES } from '../support/axe.js';
import { LOAD_MS, NAME, newElement, newPack, trialLevel, waitPackList, waitValue } from '../support/creator.js';

// H-ajc (WB-26 section 17): axe's landmark and heading rules, moderate in axe and so outside the default creator check
// (`hc-axe.e2e.ts`), run here at every impact on the screens where WB-26 found them: the title bar's quick bar outside
// any landmark (every screen), Try it's Build pickers (two "Fighter skill" regions, the Skills picker's heading level),
// and the phone PREVIEW sub-tab (the description pane's aside inside the editor region). Deterministic: no model.

const PACK = 'Ember Isles';
const PHONE = { width: 390, height: 844 } as const;

describe('hc-next landmarks and headings (axe, web mock)', { tags: ['homebrew'] }, () => {
  test('desktop: the pack list, the workspace, a feat and Try it', async (ctx) => {
    const axe = new AxeReport({ rules: LANDMARK_RULES });
    await bootToHome(ctx);
    await axe.check(ctx, 'home');
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await waitPackList(ctx);
    await axe.check(ctx, 'pack list');
    await newPack(ctx, PACK);
    await axe.check(ctx, 'workspace');
    await newElement(ctx, 'Ember Heart', /^Feat/);
    await axe.check(ctx, 'feat editor');
    await ctx.screen.getByRole('button', 'Try it').tap();
    await expect(ctx.screen.getByRole('dialog', 'Try it')).toBeVisible();
    await trialLevel(ctx, 1);
    await expect(ctx.screen.getByRole('list', 'Fighter skill options').first()).toBeVisible({ timeout: LOAD_MS });
    await axe.check(ctx, 'Try it (Build pickers)');
    axe.assertClean();
  });

  test('phone: the pack list, the editor and its PREVIEW sub-tab', async (ctx) => {
    const axe = new AxeReport({ rules: LANDMARK_RULES });
    await ctx.browser.setViewport(PHONE);
    await ctx.app.open('/?creator=1&mock=mobile#/homebrew');
    await waitPackList(ctx);
    await axe.check(ctx, 'phone pack list');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill(PACK);
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('button', 'New').first()).toBeVisible({ timeout: LOAD_MS });
    await ctx.screen.getByRole('button', 'New').first().tap();
    await ctx.browser.locator('#hb-new-name').fill('Ember Lance');
    await ctx.screen.getByRole('button', 'Create').tap();
    await waitValue(ctx, NAME, 'Ember Lance', LOAD_MS);
    await axe.check(ctx, 'phone spell editor');
    await ctx.screen.getByRole('tab', 'PREVIEW').tap();
    await expect(ctx.screen.getByRole('tab', 'PREVIEW')).toHaveAttribute('aria-selected', 'true');
    await axe.check(ctx, 'phone PREVIEW');
    axe.assertClean();
  });
});
