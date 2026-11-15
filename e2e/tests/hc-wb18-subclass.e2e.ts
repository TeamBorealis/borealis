import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { clickCss, dblClickCss, statusMsg, textOf, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-18 (spec 4.7, 18.4 on the web mock): the subclass form with the creator switched on (`?creator=1`, desktop
// 1355x847). Create a subclass, pick the SRD Fighter as its class: the subclass takes the class's subclass tag and level;
// the LEVELS grid marks the subclass feature levels; add a feature at level 3 (the subclass level, made inside the
// subclass), name it; save. The saved subclass opens again with the feature on its level. Deterministic: no model.

const NAME = '#hb-f-name-input';
const GRID = 'table[aria-label="Features by level"]';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

/** Types into the reference picker's search and waits for the list to show its answer. */
async function searchPicker(ctx: Ctx, text: string): Promise<void> {
  await ctx.browser.locator('.hb-picker .pick-fsearch').fill(text);
  await waitFor(ctx, (want: string) => {
    const rows = [...document.querySelectorAll('.hb-pick-list .pick-row')];
    return rows.length > 0 && rows.every((r) => (r.textContent ?? '').toLowerCase().includes(want));
  }, text.toLowerCase(), `picker rows matching "${text}"`);
}

async function waitFeature(ctx: Ctx, level: number, name: string): Promise<void> {
  await waitFor(ctx, ({ sel, n, want }: { sel: string; n: number; want: string }) =>
    [...document.querySelectorAll(`${sel} tr[data-level="${n}"] .hb-feature-name`)].some((b) => b.textContent === want), { sel: GRID, n: level, want: name }, `${name} at level ${level}`);
}

describe('hc-wb18 homebrew subclass form (web mock)', { tags: ['homebrew'] }, () => {
  test('create a subclass of the Fighter with a feature at its subclass level, save and reopen', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();

    await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
    await ctx.screen.getByRole('radio', /^Subclass/).tap();
    await ctx.browser.locator('#hb-new-name').fill('Ember Guard');
    await ctx.screen.getByRole('button', 'Create').tap();
    await waitValue(ctx, NAME, 'Ember Guard');
    const tabs = await ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-band [role="tab"]')].map((t) => t.textContent ?? ''), null);
    expectEqual('subclass tabs', 'GENERAL,LEVELS,SPELLCASTING,RULES,DESCRIPTION,DATA', tabs.join(','));

    // The class: its subclass choice's tag and level come with it.
    await clickCss(ctx, '[data-field="parentClass"] .hb-pick-btn');
    await expect(ctx.screen.getByRole('dialog', /^Choose: Class/)).toBeVisible();
    await searchPicker(ctx, 'fighter');
    await dblClickCss(ctx, '.hb-pick-list .pick-row[data-row="Fighter"] button.pick-main');
    await waitFor(ctx, (_: null) => document.querySelector('[data-field="subclassTag"] .hb-readonly')?.textContent === 'Fighter Subclass', null, 'the tag');
    expectEqual('chosen at level', '3', await textOf(ctx, '[data-field="levelGained"] .hb-readonly'));
    await autosaved(ctx);

    // LEVELS: the subclass feature levels marked; a feature at level 3, made inside the subclass.
    await ctx.screen.getByRole('tab', 'LEVELS').tap();
    await expect(ctx.screen.getByRole('table', 'Features by level')).toBeVisible();
    expectEqual('subclass feature levels', '3, 7, 10, 15, 18', await textOf(ctx, '[data-field="subclassLevels"] .hb-readonly'));
    const marked = await ctx.browser.evaluate((sel: string) => [...document.querySelectorAll(`${sel} tr.hb-marked-level`)].map((r) => r.getAttribute('data-level')), GRID);
    expectEqual('marked rows', '3,7,10,15,18', marked.join(','));
    await ctx.screen.getByRole('button', 'Add a feature at level 3').tap();
    await expect(ctx.screen.getByRole('dialog', /^Subclass feature:/)).toBeVisible();
    await ctx.browser.locator('.hb-child-dialog [data-field="name"] input').fill('Ember Ward');
    await expect(ctx.screen.getByRole('dialog', 'Subclass feature: Ember Ward')).toBeVisible();
    await ctx.screen.getByRole('button', 'Done').tap();
    await waitFeature(ctx, 3, 'Ember Ward');
    await autosaved(ctx);

    // Save: in the list, counted in CATEGORIES; reopened with the feature on its level.
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Ember Guard')).toBeVisible();
    expectEqual('status after save', "Saved 'Ember Guard' to Ember Isles", await statusMsg(ctx));
    await waitFor(ctx, (_: null) => /Subclasses \(1\)/.test(document.querySelector('[aria-label="Element kinds"]')?.textContent ?? ''), null, 'Subclasses (1)');
    await waitFeature(ctx, 3, 'Ember Ward');
  });
});
