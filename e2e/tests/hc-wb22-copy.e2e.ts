import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { statusMsg, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-22 (spec 11, 18.4 flow 7 on the web mock, desktop 1355x847): copy the SRD Fireball from "Start from a
// template" (SRD elements only), rename the copy (its name selected for renaming) and save it; then deep-copy the SRD
// Wizard from "New from copy..." with "Copy its features and subclasses too", save it, and its subclass is saved with it.
// The mock serves the real backend's copies (`homebrew-copies.ts`). Deterministic: no model.

const NAME = '#hb-f-name-input';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

const kindCount = (ctx: Ctx, text: RegExp, what: string): Promise<void> =>
  waitFor(ctx, (re: string) => new RegExp(re).test(document.querySelector('[aria-label="Element kinds"]')?.textContent ?? ''), text.source, what);

/** New element of `card`, started from `start`, then the picker's row `row` chosen. */
async function copyOf(ctx: Ctx, card: RegExp, start: RegExp, picker: RegExp, row: RegExp): Promise<void> {
  await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
  await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
  await ctx.screen.getByRole('radio', card).tap();
  await ctx.screen.getByRole('radio', start).tap();
  await ctx.screen.getByRole('button', 'Choose...').tap();
  await expect(ctx.screen.getByRole('dialog', picker)).toBeVisible();
  await ctx.screen.getByRole('button', row).first().tap();
  await ctx.screen.getByRole('button', 'Choose').tap();
}

describe('hc-wb22 homebrew copies (web mock)', { tags: ['homebrew'] }, () => {
  test('copy the SRD Fireball, rename and save it; deep-copy the SRD Wizard with its subclass', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();

    // Start from a template: the SRD Fireball; the copy's name is selected for renaming.
    await copyOf(ctx, /^Spell A spell/, /^Start from a template/, /Spell template/, /Fireball/);
    await waitValue(ctx, NAME, 'Fireball (copy)');
    const selected = await ctx.browser.evaluate((_: null) => {
      const box = document.activeElement as HTMLInputElement | null;
      return box?.id === 'hb-f-name-input' ? `${box.selectionStart}-${box.selectionEnd}` : '';
    }, null);
    expectEqual('name selected for renaming', `0-${'Fireball (copy)'.length}`, selected);
    await ctx.browser.locator(NAME).fill('Emberball');
    await waitValue(ctx, NAME, 'Emberball');
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await autosaved(ctx);
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Emberball')).toBeVisible();
    expectEqual('status after save', "Saved 'Emberball' to Ember Isles.", await statusMsg(ctx));
    await kindCount(ctx, /Spells \(1\)/, 'Spells (1)');

    // New from copy...: the SRD Wizard (not the sample content's), deep: its features inside, its subclass saved with it.
    await ctx.screen.getByRole('button', 'New').first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
    await ctx.screen.getByRole('radio', /^Class A class/).tap();
    await ctx.screen.getByRole('radio', /^New from copy/).tap();
    await expect(ctx.screen.getByRole('checkbox', 'Copy its features and subclasses too')).toBeChecked();
    await ctx.screen.getByRole('button', 'Choose...').tap();
    await expect(ctx.screen.getByRole('dialog', /Class to copy/)).toBeVisible();
    await ctx.screen.getByRole('button', /Wizard.*SRD 5\.2\.1/).first().tap();
    await ctx.screen.getByRole('button', 'Choose').tap();
    await waitValue(ctx, NAME, 'Wizard (copy)');
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Wizard (copy)')).toBeVisible();
    await kindCount(ctx, /Classes \(1\)/, 'Classes (1)');
    await kindCount(ctx, /Subclasses \(1\)/, 'Subclasses (1)');
  });
});
