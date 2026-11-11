import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { statusMsg, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-20 (spec 4.11, 13.4, 13.6, 18.4 on the web mock): the creature form with the creator switched on
// (`?creator=1`, desktop 1355x847). Create a 2024 creature: its type, an armor class, hit dice (the average follows),
// ability scores, a challenge rating (its XP follows) and one attack action made in the entry dialog, its text written
// from the attack; the sentences at the fields go as each is filled; the live preview reads as a stat block; save.
// Deterministic: no model.

const NAME = '#hb-f-name-input';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

describe('hc-wb20 homebrew creature form (web mock)', { tags: ['homebrew'] }, () => {
  test('create a creature with AC, HP, abilities, CR and one attack action, and save it', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();
    await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
    await ctx.screen.getByRole('radio', /^Creature A stat block/).tap();
    await ctx.browser.locator('#hb-new-name').fill('Cinder Boar');
    await ctx.screen.getByRole('button', 'Create').tap();
    await waitValue(ctx, NAME, 'Cinder Boar');

    // The stat block's tabs and GENERAL's sections, in its order; the creature type comes first.
    const tabs = await ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-band [role="tab"]')].map((t) => t.textContent ?? ''), null);
    expectEqual('tabs', 'GENERAL,STATISTICS,ACTIONS,LEGENDARY,RULES,DESCRIPTION,DATA', tabs.join(','));
    const sections = await ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-editor .hb-section-title')].map((h) => h.textContent ?? ''), null);
    expectEqual('sections', 'Size, type and alignment,Armor class and hit points,Speed and initiative,Challenge,Details', sections.join(','));
    await expect(ctx.screen.getByRole('button', /Problems \(1\)/)).toBeVisible();
    await ctx.browser.locator('#hb-f-creatureType-input').selectOption('Beast');
    // then its armor class and hit points, said at their fields
    await expect(ctx.screen.getByRole('button', /Problems \(2\)/)).toBeVisible();

    // Armor class 11 (natural armor), hit dice 2d8+2 (average 11).
    await ctx.screen.getByRole('button', 'Add an armor class').tap();
    await ctx.browser.locator('input[aria-label="Armor class 1"]').fill('11');
    await ctx.browser.locator('input[aria-label="Armor class 1: from"]').fill('natural armor');
    await ctx.browser.locator('input[aria-label="Hit points: hit dice"]').fill('2d8+2');
    await waitValue(ctx, 'input[aria-label="Hit points: average"]', '11');
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();

    // Challenge 1/4: 50 XP.
    await ctx.browser.locator('select[aria-label="Challenge rating"]').selectOption('1/4');
    await waitValue(ctx, 'input[aria-label="XP"]', '50');

    // Ability scores on STATISTICS.
    await ctx.screen.getByRole('tab', 'STATISTICS').tap();
    await ctx.browser.locator('input[aria-label="Strength"]').fill('13');
    await ctx.browser.locator('input[aria-label="Constitution"]').fill('12');
    await waitFor(ctx, (_: null) => /\+1/.test(document.querySelector('.hb-cr-ability:first-child .hb-cr-ab-mod')?.textContent ?? ''), null, 'Strength +1');

    // One attack action, its text written from the attack.
    await ctx.screen.getByRole('tab', 'ACTIONS').tap();
    await ctx.screen.getByRole('button', 'Add action').tap();
    await expect(ctx.screen.getByRole('dialog', 'New action')).toBeVisible();
    await ctx.browser.locator('.hb-cr-entry-dialog input[type="text"]').first().fill('Tusk');
    await ctx.screen.getByRole('checkbox', 'Attack').tap();
    await ctx.browser.locator('input[aria-label="Damage 1: dice"]').fill('1d6+1');
    await ctx.browser.locator('select[aria-label="Damage 1: type"]').selectOption('Piercing');
    await ctx.screen.getByRole('button', 'Write the text from the attack').tap();
    await waitFor(ctx, (_: null) => (document.querySelector('.hb-cr-entry-dialog textarea') as HTMLTextAreaElement | null)?.value
      === 'Melee Attack Roll: +3, reach 5 ft. Hit: 4 (1d6 + 1) Piercing damage.', null, 'the attack text');
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', 'Tusk')).toBeVisible();
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();

    // The live preview reads as the compendium's stat block.
    await waitFor(ctx, (_: null) => /1\/4 \(50 XP\)/.test(document.querySelector('.hb-ws-desc')?.textContent ?? ''), null, 'the challenge in the preview');
    await waitFor(ctx, (_: null) => /Tusk/.test(document.querySelector('.hb-ws-desc')?.textContent ?? ''), null, 'the action in the preview');
    await autosaved(ctx);

    // Save: in the list, counted in CATEGORIES.
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Cinder Boar')).toBeVisible();
    expectEqual('status after save', "Saved 'Cinder Boar' to Ember Isles.", await statusMsg(ctx));
    await waitFor(ctx, (_: null) => /Creatures \(1\)/.test(document.querySelector('[aria-label="Element kinds"]')?.textContent ?? ''), null, 'Creatures (1)');
  });
});
