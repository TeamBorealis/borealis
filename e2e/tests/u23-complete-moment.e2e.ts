import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import type { Ctx } from '../support/flows-a.js';

// U23 (B-143, refines TV1 C10): on a phone, creation's "Character complete" line (View sheet, Play) is a moment, not a
// fixture. It comes on Build, Magic or Gear when the session's last open pick is made, and goes when View sheet or Play
// is tapped, when it is dismissed, or when the user leaves those tabs; a character opened already complete never gets it.
// Web mock as a phone build (`?mock=mobile`) in a phone-sized page. The seeded Seraphine Vale has every selection made and
// only her starting equipment open (B-129), so declining it on Gear with "I'll buy my own" is the last pick.
// Deterministic: no model.
const PHONE = { width: 392, height: 850 } as const;
const COMPLETE = 'Character complete';

/** Opens Seraphine on a phone (Open lands on Build), where Next selection still leads to her starting equipment. */
async function openSeraphineOnPhone(ctx: Ctx): Promise<void> {
  await ctx.browser.setViewport(PHONE);
  await ctx.app.open('/?mock=mobile#/characters');
  await ctx.screen.getByRole('button', /^Seraphine Vale, Level 2/).tap();
  await ctx.screen.getByRole('button', 'Open Seraphine Vale').tap();
  await expect(ctx.screen.getByRole('heading', 'Build')).toBeAttached();
  await expect(ctx.screen.getByRole('button', /Next selection/)).toBeVisible();
  await expect(ctx.screen.getByRole('group', COMPLETE)).toBeHidden();
}

/** The last pick: Next selection opens Gear's starting equipment; "I'll buy my own" settles it. */
async function makeLastPick(ctx: Ctx): Promise<void> {
  await ctx.screen.getByRole('button', /Next selection/).tap();
  await ctx.screen.getByRole('region', 'Starting equipment').getByRole('button', /^I'll buy my own/).tap();
  await expect(ctx.screen.getByRole('group', COMPLETE)).toBeVisible();
  await expect(ctx.screen.getByRole('button', /Next selection/)).toBeHidden();
}

describe('u23 complete moment (phone)', { tags: ['u23'] }, () => {
  test('the last pick brings the moment; Dismiss takes it away and it stays away on the creation tabs', async (ctx) => {
    await openSeraphineOnPhone(ctx);
    await makeLastPick(ctx);
    await ctx.screen.getByRole('group', COMPLETE).getByRole('button', 'Dismiss').tap();
    await expect(ctx.screen.getByRole('group', COMPLETE)).toBeHidden();
    await ctx.screen.getByRole('link', 'Build').tap();
    await expect(ctx.screen.getByRole('heading', 'Build')).toBeAttached();
    await expect(ctx.screen.getByRole('group', COMPLETE)).toBeHidden();
  });

  test('View sheet opens the full-screen preview; back on Build the moment does not come back', async (ctx) => {
    await openSeraphineOnPhone(ctx);
    await makeLastPick(ctx);
    await ctx.screen.getByRole('group', COMPLETE).getByRole('button', 'View sheet').tap();
    await expect(ctx.screen.getByRole('dialog', 'Character sheet preview')).toBeVisible();
    await ctx.screen.getByRole('dialog', 'Character sheet preview').getByRole('button', 'Close').first().tap();
    await ctx.screen.getByRole('link', 'Build').tap();
    await expect(ctx.screen.getByRole('heading', 'Build')).toBeAttached();
    await expect(ctx.screen.getByRole('group', COMPLETE)).toBeHidden();
  });
});
