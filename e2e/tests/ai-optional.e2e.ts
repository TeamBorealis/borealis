import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';

// AI is optional: a fresh desktop profile has no AI entries and no setup wizard, the Settings switch reveals them,
// and a phone build never shows AI text. Deterministic: no model.
const SERAPHINE = 'Seraphine Vale, Level 2 Elf Wizard';
const AI_TEXT = /\bAI\b|assistant|codex|claude|\bpair/i;

const pageText = (ctx: any): Promise<string> => ctx.browser.evaluate((_: null) => document.body.innerText, null);
const hasAiText = async (ctx: any): Promise<boolean> => AI_TEXT.test(await pageText(ctx));

async function menuHasAi(ctx: any, menu: string): Promise<boolean> {
  await ctx.screen.getByRole('menuitem', menu).tap();
  const found = await hasAiText(ctx);
  await pressKey(ctx, 'Escape');
  return found;
}

async function openBorealisSettings(ctx: any): Promise<void> {
  await ctx.screen.getByRole('menuitem', 'View').tap();
  await ctx.screen.getByRole('menuitem', 'Borealis Settings').tap();
  await expect(ctx.screen.getByRole('dialog', 'Borealis Settings')).toBeVisible();
}

describe('ai-optional', { tags: ['ai-optional'] }, () => {
  test('fresh desktop profile: no AI entries and no setup wizard', async (ctx) => {
    await bootToHome(ctx);
    await expect(ctx.screen.getByRole('button', SERAPHINE)).toBeVisible();
    expect(await hasAiText(ctx)).toBe(false);
    expect(await menuHasAi(ctx, 'View')).toBe(false);
    expect(await menuHasAi(ctx, 'Help')).toBe(false);
    // AI routes are not reachable while AI is off.
    for (const route of ['/#/ai', '/#/setup/ai', '/#/import']) {
      await ctx.app.open(route);
      await expect(ctx.screen.getByRole('button', SERAPHINE)).toBeVisible();
      expect(await hasAiText(ctx)).toBe(false);
    }
  });

  test('the Settings switch reveals the AI entries', async (ctx) => {
    await bootToHome(ctx);
    await openBorealisSettings(ctx);
    const toggle = ctx.screen.getByRole('checkbox', 'Enable AI assistant (optional)');
    await expect(toggle).toBeVisible();
    await toggle.tap();
    await expect(ctx.screen.getByLabel('AI PROVIDER')).toBeVisible();
    await ctx.screen.getByRole('button', 'Close').last().tap();
    await ctx.screen.getByRole('menuitem', 'View').tap();
    await expect(ctx.screen.getByRole('menuitem', 'AI Assistant')).toBeVisible();
    await pressKey(ctx, 'Escape');
    await ctx.screen.getByRole('menuitem', 'Help').tap();
    await expect(ctx.screen.getByRole('menuitem', 'AI Setup')).toBeVisible();
  });

  test('phone build: no AI text anywhere', async (ctx) => {
    await ctx.app.open('/?mock=mobile#/characters');
    await expect(ctx.screen.getByRole('button', SERAPHINE)).toBeVisible();
    expect(await hasAiText(ctx)).toBe(false);
    for (const route of ['/?mock=mobile#/ai', '/?mock=mobile#/setup/ai', '/?mock=mobile#/more']) {
      await ctx.app.open(route);
      await new Promise((r) => setTimeout(r, 400));
      expect(await hasAiText(ctx)).toBe(false);
    }
  });
});
