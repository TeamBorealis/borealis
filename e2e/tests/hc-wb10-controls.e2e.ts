import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { clickCss, dblClickCss, textOf, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-10 (spec 5, 6, 7, 13.5-13.7, 13.13, 16): the creator's controls on the web mock with the creator switched on
// (`?creator=1`): a prerequisite from the condition builder and the reference picker, rule blocks from the Add menu and
// the generic block dialog (a spell from the spell picker's Magic grid, a statistic from the statistic picker), the
// description in the rich-text editor, Save; then the same controls on a phone (`?mock=mobile`, 392x850): bottom sheets,
// U22 select sheets, Choose under the description. Deterministic: no model.

const NAME = '#hb-f-name-input';
const PHONE = { width: 392, height: 850 } as const;

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

/**
 * Types into the reference picker's search and waits for the list to show its answer: the query goes out once typing
 * pauses (200 ms), so until then the unfiltered list is still on screen with the row in another place (or far below).
 */
async function searchPicker(ctx: Ctx, text: string): Promise<void> {
  await ctx.browser.locator('.hb-picker .pick-fsearch').fill(text);
  await waitFor(ctx, (want: string) => {
    const rows = [...document.querySelectorAll('.hb-pick-list .pick-row')];
    return rows.length > 0 && rows.every((r) => (r.textContent ?? '').toLowerCase().includes(want));
  }, text.toLowerCase(), `picker rows matching "${text}"`);
}

/** START > HOMEBREW > NEW PACK "Ember Isles" > New element (a spell) "Ember Bolt". */
async function newSpell(ctx: Ctx, phone: boolean): Promise<void> {
  await ctx.screen.getByRole('button', 'New pack').first().tap();
  await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
  await ctx.screen.getByRole('button', 'Create').tap();
  await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();
  await ctx.screen.getByRole('button', phone ? 'New' : 'New element').first().tap();
  await ctx.browser.locator('#hb-new-name').fill('Ember Bolt');
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, 'Ember Bolt');
}

describe('hc-wb10 homebrew creator controls (web mock)', { tags: ['homebrew'] }, () => {
  test('desktop: prerequisite, rule blocks with the spell and statistic pickers, description, save', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await newSpell(ctx, false);
    // A spell needs a school to be saved (WB-11).
    await ctx.browser.locator('#hb-f-school-input').selectOption('Evocation');

    // GENERAL: a prerequisite row "Has <Wizard>" from the condition builder and the reference picker.
    await ctx.screen.getByRole('button', 'Add condition').tap();
    // (PROBLEMS is open: a new spell asked for its school at first, so the sentence is also listed there)
    await expect(ctx.screen.getByText('Pick the element this condition tests.').first()).toBeVisible();
    await clickCss(ctx, '[data-field="requirements"] .hb-pick-btn');
    await expect(ctx.screen.getByRole('dialog', /^Choose: Prerequisite/)).toBeVisible();
    await searchPicker(ctx, 'wizard');
    await dblClickCss(ctx, '.hb-pick-list .pick-row[data-row="Wizard"] button.pick-main');
    await expect(ctx.screen.getByText('Pick the element this condition tests.').first()).toBeHidden();
    expectEqual('the row names the element', 'Wizard', await textOf(ctx, '[data-field="requirements"] .hb-ref-name'));

    // RULES: Insert opens the Add menu; "Gives a spell" -> the spell picker's Magic grid, level 3 -> Fireball.
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await pressKey(ctx, 'Insert');
    await expect(ctx.screen.getByRole('dialog', 'Add a rule')).toBeVisible();
    await ctx.screen.getByRole('button', 'Gives a spell').tap();
    await expect(ctx.screen.getByRole('dialog', 'New rule: Gives a spell')).toBeVisible();
    await ctx.screen.getByRole('button', 'Pick: Spell').tap();
    await expect(ctx.screen.getByRole('table', 'Spells')).toBeVisible();
    await clickCss(ctx, '.hb-spell-picker .sp-levels button:nth-child(4)');
    await waitFor(ctx, (_: null) => document.querySelector('.hb-spell-picker [data-spell-id="fire-bolt"]') === null, null, 'level 3 filter');
    await ctx.browser.locator('.hb-spell-picker input[aria-label="Choose Fireball"]').check();
    await expect(ctx.screen.getByRole('table', 'Spells')).toBeHidden();
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', /Gives a spell \(Fireball/)).toBeVisible();

    // "Changes a number" -> the statistic picker -> Darkvision range.
    await ctx.screen.getByRole('button', 'Add rule').tap();
    await ctx.screen.getByRole('button', 'Changes a number').tap();
    await ctx.screen.getByRole('button', 'Pick: Statistic').tap();
    await ctx.browser.locator('input[aria-label="Search statistics"]').fill('darkvision');
    await ctx.screen.getByRole('button', 'Darkvision range').tap();
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', /Changes a number \(Darkvision range/)).toBeVisible();
    await autosaved(ctx);

    // Undo takes the last rule back; Redo restores it.
    await pressKey(ctx, 'Control+z');
    await expect(ctx.screen.getByRole('listitem', /Changes a number/)).toBeHidden();
    await pressKey(ctx, 'Control+Shift+z');
    await expect(ctx.screen.getByRole('listitem', /Changes a number \(Darkvision range/)).toBeVisible();

    // DESCRIPTION: the rich-text editor; the live description pane follows.
    await ctx.screen.getByRole('tab', 'DESCRIPTION').tap();
    await expect(ctx.screen.getByRole('toolbar', 'Description formatting')).toBeVisible();
    await ctx.browser.locator('.ProseMirror').fill('A mote of ember fire.');
    await ctx.screen.getByRole('button', /higher-level/i).first().tap();
    await autosaved(ctx);
    await waitFor(ctx, (_: null) => /A mote of ember fire\./.test(document.querySelector('.hb-ws-desc')?.textContent ?? ''), null, 'preview text');
    await waitFor(ctx, (_: null) => /Using a Higher-Level Spell Slot\./.test(document.querySelector('.hb-ws-desc')?.textContent ?? ''), null, 'preview higher-level');

    // Save: the element is in the list.
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Ember Bolt')).toBeVisible();
  });

  test('phone: sheets for the Add menu, the block dialog (U22 selects) and the reference picker', async (ctx) => {
    await ctx.browser.setViewport(PHONE);
    await ctx.app.open('/?creator=1&mock=mobile#/homebrew');
    await expect(ctx.screen.getByRole('button', 'New pack').first()).toBeVisible();
    await newSpell(ctx, true);
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await ctx.screen.getByRole('button', 'Add rule').tap();
    await ctx.screen.getByRole('button', 'Advantage on a roll').tap();
    await expect(ctx.screen.getByRole('dialog', 'New rule: Advantage on a roll')).toBeVisible();
    // The Roll select is the app's bottom-sheet list, not the WebView's select (U22).
    // The sheet rises (180 ms): the locator's click waits until the select stops moving.
    await ctx.browser.locator('[data-field="roll"] .ph-select').click();
    await ctx.screen.getByRole('option', 'Saving throws').tap();
    expectEqual('roll chosen in the sheet', 'Saving throws', await textOf(ctx, '[data-field="roll"] .ph-select-value'));
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', /Advantage on a roll \(Saving throws/)).toBeVisible();

    // "Gives ..." -> the reference picker as a sheet; Choose under the description (O2 B).
    await ctx.screen.getByRole('button', 'Add rule').tap();
    await ctx.screen.getByRole('button', 'Gives ...').tap();
    await ctx.screen.getByRole('button', 'Pick: What it gives').tap();
    await searchPicker(ctx, 'stealth');
    await ctx.browser.locator('.hb-pick-list .pick-row[data-row="Stealth"] button.pick-main').click();
    await ctx.screen.getByRole('button', 'Choose Stealth').last().tap();
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', /Gives \.\.\. \(Stealth/)).toBeVisible();
    await autosaved(ctx);
  });
});
