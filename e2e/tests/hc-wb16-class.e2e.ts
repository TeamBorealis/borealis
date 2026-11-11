import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { statusMsg, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-16 (spec 4.5, 13.8, 18.4 flow 3 on the web mock): the class form with the creator switched on
// (`?creator=1`, desktop 1355x847). Create a 2024 class with a d10; the LEVELS grid shows its generated features
// (subclass choice at 3, ability score improvements from 4, epic boon at 19); add a feature at level 1 from the grid
// (made inside the class), name it and give it a rule block in the feature editor; save. The saved class opens again
// with the feature on its level (the save reopens it). Deterministic: no model.

const NAME = '#hb-f-name-input';
const GRID = 'table[aria-label="Features by level"]';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

/** The feature names on a level of the grid. */
const featuresAt = (ctx: Ctx, level: number): Promise<string[]> =>
  ctx.browser.evaluate(({ sel, n }: { sel: string; n: number }) =>
    [...document.querySelectorAll(`${sel} tr[data-level="${n}"] .hb-feature-name`)].map((b) => b.textContent ?? ''), { sel: GRID, n: level });

async function waitFeature(ctx: Ctx, level: number, name: string): Promise<void> {
  await waitFor(ctx, ({ sel, n, want }: { sel: string; n: number; want: string }) =>
    [...document.querySelectorAll(`${sel} tr[data-level="${n}"] .hb-feature-name`)].some((b) => b.textContent === want), { sel: GRID, n: level, want: name }, `${name} at level ${level}`);
}

describe('hc-wb16 homebrew class form (web mock)', { tags: ['homebrew'] }, () => {
  test('create a 2024 class, add a level-1 feature with a rule block from the LEVELS grid, save and reopen', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();

    await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
    await ctx.screen.getByRole('radio', /^Class A class/).tap();
    await ctx.browser.locator('#hb-new-name').fill('Gloom Knight');
    await ctx.screen.getByRole('button', 'Create').tap();
    await waitValue(ctx, NAME, 'Gloom Knight');
    await ctx.browser.locator('#hb-f-hitDie-input').selectOption('d10');

    // The class's sub-tabs, then the LEVELS grid with the generated features.
    const tabs = await ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-band [role="tab"]')].map((t) => t.textContent ?? ''), null);
    expectEqual('class tabs', 'GENERAL,PROFICIENCIES,LEVELS,SPELLCASTING,MULTICLASS,EQUIPMENT,RULES,DESCRIPTION', tabs.join(','));
    await ctx.screen.getByRole('tab', 'LEVELS').tap();
    await expect(ctx.screen.getByRole('table', 'Features by level')).toBeVisible();
    expectEqual('level 3', 'Subclass', (await featuresAt(ctx, 3)).join(','));
    expectEqual('level 4', 'Ability Score Improvement', (await featuresAt(ctx, 4)).join(','));
    expectEqual('level 19', 'Epic Boon', (await featuresAt(ctx, 19)).join(','));

    // A feature at level 1, made inside the class; the feature editor names it and gives it a rule block.
    await ctx.screen.getByRole('button', 'Add a feature at level 1').tap();
    await expect(ctx.screen.getByRole('dialog', /^Class feature:/)).toBeVisible();
    await ctx.browser.locator('.hb-child-dialog [data-field="name"] input').fill('Shadow Step');
    await expect(ctx.screen.getByRole('dialog', 'Class feature: Shadow Step')).toBeVisible();
    await ctx.screen.getByRole('button', 'Add rule').tap();
    await expect(ctx.screen.getByRole('dialog', 'Add a rule')).toBeVisible();
    await ctx.screen.getByRole('button', 'Advantage on a roll').tap();
    await expect(ctx.screen.getByRole('dialog', 'New rule: Advantage on a roll')).toBeVisible();
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', /Advantage on a roll/)).toBeVisible();
    await ctx.screen.getByRole('button', 'Done').tap();
    await waitFeature(ctx, 1, 'Shadow Step');
    await autosaved(ctx);

    // Save: in the list, counted in CATEGORIES.
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Gloom Knight')).toBeVisible();
    expectEqual('status after save', "Saved 'Gloom Knight' to Ember Isles.", await statusMsg(ctx));
    await waitFor(ctx, (_: null) => /Classes \(1\)/.test(document.querySelector('[aria-label="Element kinds"]')?.textContent ?? ''), null, 'Classes (1)');

    // After the save the class is opened again from the pack: the feature is on its level with its rule.
    await waitFeature(ctx, 1, 'Shadow Step');
    await ctx.browser.locator(`${GRID} tr[data-level="1"] .hb-feature-name`).first().click();
    await expect(ctx.screen.getByRole('dialog', 'Class feature: Shadow Step')).toBeVisible();
    await expect(ctx.screen.getByRole('listitem', /Advantage on a roll/)).toBeVisible();
    await ctx.screen.getByRole('button', 'Done').tap();
  });
});
