import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { dblClickCss, statusMsg, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-14 (spec 4.3, 13.4, 18.4 on the web mock): the background form with the creator switched on
// (`?creator=1`, desktop 1355x847). Create a 2024 background: three abilities, an origin feat (Magic Initiate with
// the Druid spell list, one of the feat's own choices) and two skills; the sentences at the fields go as each is
// filled (Problems (3) -> (0)); save. Deterministic: no model.

const NAME = '#hb-f-name-input';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

/** In the open picker, search for `name`, wait for its row alone, and double-click it. */
async function pickRow(ctx: Ctx, name: string): Promise<void> {
  await ctx.browser.locator('.hb-picker .pick-fsearch').fill(name.toLowerCase());
  const row = `.hb-pick-list .pick-row[data-row="${name}"]`;
  await waitFor(ctx, ({ sel }: { sel: string }) => {
    const rows = [...document.querySelectorAll('.hb-pick-list .pick-row')];
    return rows.length === 1 && rows[0] === document.querySelector(sel);
  }, { sel: row }, `only ${name} offered`);
  await dblClickCss(ctx, `${row} button.pick-main`);
}

describe('hc-wb14 homebrew background form (web mock)', { tags: ['homebrew'] }, () => {
  test('create a 2024 background with three abilities, an origin feat and two skills, and save it', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();
    await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
    await ctx.screen.getByRole('radio', /^Background A background/).tap();
    await ctx.browser.locator('#hb-new-name').fill('Ember Warden');
    await ctx.screen.getByRole('button', 'Create').tap();
    await waitValue(ctx, NAME, 'Ember Warden');

    // The 2024 form: its sections and tabs (no 2014 characteristics); what it still needs, at its fields.
    const sections = await ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-editor .hb-section-title')].map((h) => h.textContent ?? ''), null);
    expectEqual('sections', 'Ability scores,Origin feat,Proficiencies,Details', sections.join(','));
    const tabs = await ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-band [role="tab"]')].map((t) => t.textContent ?? ''), null);
    expectEqual('tabs', 'GENERAL,EQUIPMENT,RULES,DESCRIPTION', tabs.join(','));
    await expect(ctx.screen.getByRole('button', /Problems \(3\)/)).toBeVisible();

    // Three abilities.
    for (const a of ['Strength', 'Constitution', 'Wisdom']) {
      await ctx.screen.getByRole('button', a).tap();
      await expect(ctx.screen.getByRole('button', a)).toHaveAttribute('aria-pressed', 'true');
    }
    await expect(ctx.screen.getByRole('button', /Problems \(2\)/)).toBeVisible();

    // The origin feat, with the feat's own spell list made for the character.
    await ctx.screen.getByRole('button', 'Pick: Origin feat').tap();
    await expect(ctx.screen.getByRole('dialog', /^Choose: Origin feat/)).toBeVisible();
    await pickRow(ctx, 'Magic Initiate');
    const list = 'select[aria-label="Spell List of the feat"]';
    await waitFor(ctx, (sel: string) => document.querySelector(sel) !== null, list, 'the Spell List choice');
    await ctx.browser.locator(list).selectOption('Druid');
    await waitValue(ctx, list, 'Druid');
    await expect(ctx.screen.getByRole('button', /Problems \(1\)/)).toBeVisible();

    // Two skills.
    await ctx.screen.getByRole('button', 'Pick: Skill proficiencies').tap();
    await expect(ctx.screen.getByRole('dialog', 'Choose proficiencies')).toBeVisible();
    await pickRow(ctx, 'Insight');
    await pickRow(ctx, 'Religion');
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('list', 'Skill proficiencies')).toContainText('Religion');
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await autosaved(ctx);

    // Save: in the list, counted in CATEGORIES; it opens again with its choices.
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Ember Warden')).toBeVisible();
    expectEqual('status after save', "Saved 'Ember Warden' to Ember Isles.", await statusMsg(ctx));
    await waitFor(ctx, (_: null) => /Backgrounds \(1\)/.test(document.querySelector('[aria-label="Element kinds"]')?.textContent ?? ''), null, 'Backgrounds (1)');
    await ctx.screen.getByRole('button', 'Ember Warden').tap();
    await waitValue(ctx, NAME, 'Ember Warden');
    await waitValue(ctx, list, 'Druid');
    await expect(ctx.screen.getByRole('button', 'Wisdom')).toHaveAttribute('aria-pressed', 'true');
  });
});
