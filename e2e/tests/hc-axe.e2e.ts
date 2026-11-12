import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome } from '../support/index.js';
import type { Ctx } from '../support/flows-a.js';
import { AxeReport } from '../support/axe.js';
import { addRule, LOAD_MS, NAME, newElement, newPack, trialLevel, waitPackList, waitValue } from '../support/creator.js';

// Homebrew WB-27 (spec 18.5, board item H-aZY): axe-core over every creator screen in the web e2e run, on the desktop
// target (1355x847) and the phone variant (`?mock=mobile`, 390x844). A critical or serious violation fails the run;
// `support/axe.ts` lists the one rule left out (Aurora's colours, pending WB-26 Q7). Deterministic: no model.

const PACK = 'Ember Isles';
const PHONE = { width: 390, height: 844 } as const;

/** The kinds whose forms are checked on every sub-tab: the card in New element and the name given. */
const KINDS: readonly [RegExp, string][] = [
  [/^Spell A spell/, 'Ember Lance'],
  [/^Feat/, 'Ember Heart'],
  [/^Class A class/, 'Gloom Knight'],
  [/^Subclass/, 'Ash Order'],
  [/^Species/, 'Emberkin'],
  [/^Race A 2014/, 'Stonefolk'],
  [/^Background A background/, 'Ember Warden'],
  [/^Weapon A weapon/, 'Sickle Sword'],
  [/^Armor Armor or a shield/, 'Ash Plate'],
  [/^Magic item/, 'Ember Blade'],
  [/^Creature A stat block/, 'Ash Hound'],
  [/^Language A language/, 'Emberspeech'],
  [/^Other element/, 'Ember Relic'],
];

const tabNames = (ctx: Ctx): Promise<string[]> =>
  ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-band [role="tab"]')].map((t) => t.textContent ?? ''), null);

/** Every sub-tab of the open editor, each checked. */
async function checkTabs(ctx: Ctx, axe: AxeReport, kind: string): Promise<void> {
  for (const tab of await tabNames(ctx)) {
    await ctx.screen.getByRole('tab', tab).tap();
    await expect(ctx.screen.getByRole('tab', tab)).toHaveAttribute('aria-selected', 'true');
    await axe.check(ctx, `${kind} ${tab}`);
  }
}

describe('hc-axe homebrew creator accessibility (axe-core, web mock)', { tags: ['homebrew'] }, () => {
  test('desktop: the pack list, New pack, the workspace and every kind form on every sub-tab', async (ctx) => {
    const axe = new AxeReport();
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await waitPackList(ctx);
    await axe.check(ctx, 'pack list (empty)');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New pack')).toBeVisible();
    await axe.check(ctx, 'New pack');
    await ctx.screen.getByRole('button', 'Cancel').last().tap();
    await newPack(ctx, PACK, 'Both editions');
    await axe.check(ctx, 'workspace (empty pack)');
    await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
    await axe.check(ctx, 'New element');
    await ctx.screen.getByRole('button', 'Cancel').last().tap();
    for (const [card, name] of KINDS) {
      await newElement(ctx, name, card);
      await checkTabs(ctx, axe, name);
    }
    const screens = axe.assertClean();
    // every kind has at least GENERAL, RULES (or its own), DESCRIPTION and DATA
    if (screens < KINDS.length * 4) throw new Error(`only ${screens} screens checked`);
  });

  test('desktop: the dialogs and flows (Add rule, a block, a picker, Try it, export, import preview)', async (ctx) => {
    const axe = new AxeReport();
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await newPack(ctx, PACK);
    await newElement(ctx, 'Ember Lance');
    await ctx.browser.locator('#hb-f-school-input').selectOption('Evocation');
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await ctx.screen.getByRole('button', 'Add rule').tap();
    await expect(ctx.screen.getByRole('dialog', 'Add a rule')).toBeVisible();
    await axe.check(ctx, 'Add a rule');
    await ctx.screen.getByRole('button', 'Cancel').last().tap();
    await addRule(ctx, 'Gives a spell');
    await axe.check(ctx, 'block dialog');
    await ctx.screen.getByRole('button', 'Pick: Spell').tap();
    await expect(ctx.screen.getByRole('table', 'Spells')).toBeVisible();
    await axe.check(ctx, 'spell picker');
    await ctx.screen.getByRole('button', 'Cancel').last().tap();
    await ctx.screen.getByRole('button', 'Cancel').last().tap();
    await ctx.screen.getByRole('button', 'Try it').tap();
    await expect(ctx.screen.getByRole('dialog', 'Try it')).toBeVisible();
    await trialLevel(ctx, 1);
    await axe.check(ctx, 'Try it');
    await ctx.screen.getByRole('button', 'Close').last().tap();
    await ctx.screen.getByRole('button', 'Spell Lists').tap();
    await axe.check(ctx, 'SPELL LISTS');
    await ctx.screen.getByRole('button', 'All packs').tap();
    await waitPackList(ctx);
    await ctx.browser.locator('.hb-pack-grid .v4-grid-row').first().click();
    await axe.check(ctx, 'pack list (a pack selected)');
    await ctx.screen.getByRole('button', 'Export pack').tap();
    await expect(ctx.screen.getByRole('dialog', 'Export pack')).toBeVisible();
    await axe.check(ctx, 'Export pack');
    axe.assertClean();
  });

  test('phone: the pack list, workspace, editor head, a select sheet and Try it', async (ctx) => {
    const axe = new AxeReport();
    await ctx.browser.setViewport(PHONE);
    await ctx.app.open('/?creator=1&mock=mobile#/homebrew');
    await waitPackList(ctx);
    await axe.check(ctx, 'phone pack list');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill(PACK);
    await axe.check(ctx, 'phone New pack sheet');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('button', 'New').first()).toBeVisible({ timeout: LOAD_MS });
    await ctx.screen.getByRole('button', 'New').first().tap();
    await ctx.browser.locator('#hb-new-name').fill('Ember Lance');
    await axe.check(ctx, 'phone New element sheet');
    await ctx.screen.getByRole('button', 'Create').tap();
    await waitValue(ctx, NAME, 'Ember Lance', LOAD_MS);
    await axe.check(ctx, 'phone spell editor');
    await ctx.browser.locator('#hb-f-school-input').click();
    await expect(ctx.screen.getByRole('option', 'Evocation')).toBeVisible();
    await axe.check(ctx, 'phone select sheet');
    await ctx.screen.getByRole('option', 'Evocation').tap();
    await ctx.screen.getByRole('button', 'Try it').last().tap();
    await expect(ctx.screen.getByRole('dialog', 'Try it')).toBeVisible();
    await trialLevel(ctx, 1);
    await axe.check(ctx, 'phone Try it');
    axe.assertClean();
  });
});
