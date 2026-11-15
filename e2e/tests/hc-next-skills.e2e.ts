import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { statusMsg, waitFor, type Ctx } from '../support/flows-a.js';

// HC16 (2), item IpY1E, on the web mock (desktop 1355x847): the creator's "Advantage on a roll" block offers the
// installed homebrew skills after the eighteen. A Skill "Alchemy" is saved; a feat's "Advantage on a roll" block lists
// it last in its Skill list and takes it; the feat is saved and opens again with the block still naming Alchemy, its
// dialog showing Alchemy picked. Deterministic: no model.

const NAME = '#hb-f-name-input';
const SKILL = '[data-field="skill"] select';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

async function newElement(ctx: Ctx, kind: RegExp, name: string): Promise<void> {
  await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
  await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
  await ctx.screen.getByRole('radio', kind).tap();
  await ctx.browser.locator('#hb-new-name').fill(name);
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, name);
}

/** The Skill list's options, once it lists `last` (the installed skills come from a second answer). */
async function skillOptions(ctx: Ctx, last: string): Promise<string[]> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => {
    const options = [...(document.querySelector(sel) as HTMLSelectElement | null)?.options ?? []];
    return options.at(-1)?.textContent === v;
  }, { sel: SKILL, v: last }, `the Skill list ends with ${last}`);
  return ctx.browser.evaluate((sel: string) => [...(document.querySelector(sel) as HTMLSelectElement).options].map((o) => o.textContent ?? ''), SKILL);
}

describe('hc-next-skills the skill blocks offer homebrew skills (web mock)', { tags: ['homebrew'] }, () => {
  test('a feat\'s Advantage on a roll picks an installed homebrew skill, saves and opens again with it', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Old World');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Old World')).toBeAttached();

    // the skill, saved: installed
    await newElement(ctx, /^Skill A skill/, 'Alchemy');
    await ctx.browser.locator('#hb-f-ability-input').fill('Intelligence');
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await autosaved(ctx);
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Alchemy')).toBeVisible();

    // a feat: Advantage on a roll, on Alchemy checks
    await newElement(ctx, /^Feat/, "Brewer's Knack");
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await ctx.screen.getByRole('button', 'Add rule').tap();
    await expect(ctx.screen.getByRole('dialog', 'Add a rule')).toBeVisible();
    await ctx.screen.getByRole('button', 'Advantage on a roll').tap();
    await expect(ctx.screen.getByRole('dialog', 'New rule: Advantage on a roll')).toBeVisible();
    const options = await skillOptions(ctx, 'Alchemy');
    expectEqual('the eighteen, then Alchemy', 19, options.filter((o) => o !== 'Not set').length);
    expectEqual('Stealth still offered', 'yes', options.includes('Stealth') ? 'yes' : 'no');
    await ctx.browser.locator(SKILL).selectOption('Alchemy');
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', /Advantage on a roll.*Alchemy/)).toBeVisible();
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await autosaved(ctx);
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', "Brewer's Knack")).toBeVisible();
    expectEqual('status after the feat', "Saved 'Brewer's Knack' to Old World", await statusMsg(ctx));

    // opened again: the block still names Alchemy, and its dialog has it picked
    await ctx.screen.getByRole('button', /^Skills \(1\)/).tap();
    await ctx.screen.getByRole('button', 'Alchemy').tap();
    await waitValue(ctx, NAME, 'Alchemy');
    await ctx.screen.getByRole('button', /^Feats \(1\)/).tap();
    await ctx.screen.getByRole('button', "Brewer's Knack").tap();
    await waitValue(ctx, NAME, "Brewer's Knack");
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await expect(ctx.screen.getByRole('listitem', /Advantage on a roll.*Alchemy/)).toBeVisible();
    await ctx.screen.getByRole('button', /^Edit: Advantage on a roll/).tap();
    await expect(ctx.screen.getByRole('dialog', /Advantage on a roll/)).toBeVisible();
    await skillOptions(ctx, 'Alchemy');
    await waitFor(ctx, (sel: string) => (document.querySelector(sel) as HTMLSelectElement | null)?.value === 'Alchemy', SKILL, 'Alchemy picked');
  });
});
