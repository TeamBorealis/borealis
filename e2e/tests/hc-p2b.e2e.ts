import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual, settledBox } from '../support/index.js';
import { dblClickCss, textOf, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew P2b (1.5.0, the web mock, desktop 1355x847): a new weapon's live preview gives every weapon fact as its own
// block after the description, and Try it carries and equips it as an attack (not a feature, not attuned); a creature's
// live preview is the whole stat block; Edit on a loaded element in WHERE USED opens a copy of it in the pack.
// Deterministic: no model.

const NAME = '#hb-f-name-input';
const PANE = '.hb-ws-desc';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

/** Scrolls an element into view and waits until it holds still (the hit-tested click helpers test its centre). */
async function reveal(ctx: Ctx, css: string): Promise<void> {
  await ctx.browser.evaluate((sel: string) => {
    const el = document.querySelector(sel);
    el?.scrollIntoView({ block: 'center' });
    return el !== null;
  }, css);
  await settledBox(ctx.browser.locator(css).first());
}

async function newPack(ctx: Ctx): Promise<void> {
  await bootToHome(ctx, '/?creator=1#/homebrew');
  await ctx.screen.getByRole('button', 'New pack').first().tap();
  await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
  await ctx.screen.getByRole('button', 'Create').tap();
  await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();
}

async function newElement(ctx: Ctx, kind: RegExp, name: string): Promise<void> {
  await ctx.screen.getByRole('button', 'New element').first().tap();
  await ctx.browser.locator('#hb-new-name').fill(name);
  await ctx.screen.getByRole('radio', kind).tap();
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, name);
}

/** Pick the row `row` for the reference field `field` (searched by `query`). */
async function pick(ctx: Ctx, field: string, query: string, row: string): Promise<void> {
  await reveal(ctx, `[data-field="${field}"] .hb-pick-btn`);
  await ctx.browser.locator(`[data-field="${field}"] .hb-pick-btn`).click();
  await expect(ctx.screen.getByRole('dialog', /^Choose/)).toBeVisible();
  await ctx.browser.locator('.hb-picker .pick-fsearch').fill(query);
  const css = `.hb-pick-list .pick-row[data-row="${row}"] button.pick-main`;
  await waitFor(ctx, (sel: string) => document.querySelector(sel) !== null, css, `the ${row} row`);
  await settledBox(ctx.browser.locator(css).first());
  await dblClickCss(ctx, css);
  const open = await ctx.browser.evaluate((_: null) => document.querySelector('.hb-picker') !== null, null);
  if (open) await ctx.screen.getByRole('button', 'OK').tap();
  await expect(ctx.screen.getByRole('dialog', /^Choose/)).toBeHidden();
  await waitFor(ctx, ({ f, r }: { f: string; r: string }) => (document.querySelector(`[data-field="${f}"]`)?.textContent ?? '').includes(r), { f: field, r: row }, `${field} holds ${row}`);
}

const paneFacts = (ctx: Ctx): Promise<string[]> =>
  ctx.browser.evaluate((p: string) => [...document.querySelectorAll(`${p} .desc-facts .desc-line`)].map((x) => x.textContent ?? ''), PANE);

describe('hc-p2b homebrew weapon, creature preview and Edit a copy (web mock)', { tags: ['homebrew'] }, () => {
  test('a weapon: every fact in the preview, its own block; Try it carries and equips it as an attack', async (ctx) => {
    await newPack(ctx);
    await newElement(ctx, /^Weapon A weapon/, 'Twinblade');
    await ctx.browser.locator('#hb-f-damage-input').fill('2d4');
    await pick(ctx, 'damageType', 'slash', 'Slashing');
    await pick(ctx, 'mastery', 'vex', 'Vex');
    await reveal(ctx, '#hb-f-cost-input');
    await ctx.browser.locator('#hb-f-cost-input').fill('50');
    await ctx.browser.locator('#hb-f-weight-input').fill('6');
    await ctx.screen.getByRole('tab', 'DESCRIPTION').tap();
    await expect(ctx.screen.getByRole('toolbar', 'Description formatting')).toBeVisible();
    await ctx.browser.locator('.ProseMirror').fill('Two curved blades share one long grip.');
    await autosaved(ctx);
    await waitFor(ctx, (p: string) => /Two curved blades/.test(document.querySelector(p)?.textContent ?? ''), PANE, 'preview text');

    await waitFor(ctx, (p: string) => [...document.querySelectorAll(`${p} .desc-facts .desc-line`)].some((x) => /^Weight\. 6 lb\.$/.test(x.textContent ?? '')), PANE, 'the weight fact');
    const facts = await paneFacts(ctx);
    for (const want of ['Cost. 50 gp', 'Damage. 2d4 slashing', 'Mastery. Vex', 'Weight. 6 lb.']) {
      if (!facts.includes(want)) throw new Error(`the preview has no "${want}": ${facts.join(' | ')}`);
    }
    // the facts are their own block, apart from the description above them
    const gap = await ctx.browser.evaluate((p: string) => {
      const f = document.querySelector(`${p} .desc-facts`);
      return f ? getComputedStyle(f).marginTop : '';
    }, PANE);
    expectEqual('the facts block keeps apart from the text', '12px', gap);

    await ctx.screen.getByRole('button', 'Try it').tap();
    await expect(ctx.screen.getByRole('dialog', 'Try it')).toBeVisible();
    await waitFor(ctx, (_: null) => /Your test character is level 1/i.test(document.querySelector('.hb-try')?.textContent ?? ''), null, 'the trial at level 1');
    await expect(ctx.screen.getByText('Added to the inventory and equipped.')).toBeVisible();
    await waitFor(ctx, (_: null) => /Twinblade/.test(document.querySelector('.hb-try [aria-label="Attacks"]')?.textContent ?? ''), null, 'Twinblade an attack');
    expectEqual('never a feature', 'no', /Twinblade/.test(await textOf(ctx, '.hb-try [aria-label="Features"]')) ? 'yes' : 'no');
    expectEqual('never attuned', 'no', /attuned/.test(await textOf(ctx, '.hb-try-draft')) ? 'yes' : 'no');
    await ctx.screen.getByRole('button', 'Close').last().tap();
    await expect(ctx.screen.getByRole('dialog', 'Try it')).toBeHidden();
  });

  test("a creature's live preview is the whole stat block", async (ctx) => {
    await newPack(ctx);
    await newElement(ctx, /^Creature A stat block/, 'Cinder Boar');
    await ctx.browser.locator('#hb-f-creatureType-input').selectOption('Beast');
    await autosaved(ctx);
    await waitFor(ctx, (p: string) => document.querySelectorAll(`${p} th`).length === 6, PANE, 'the six scores');
    const heads = await ctx.browser.evaluate((p: string) => [...document.querySelectorAll(`${p} th`)].map((x) => x.textContent ?? '').join(' '), PANE);
    expectEqual('the scores', 'STR DEX CON INT WIS CHA', heads);
    const text = await textOf(ctx, PANE);
    for (const want of ['beast', 'Armor Class.', 'Hit Points.', 'Senses.', 'passive Perception', 'Languages.', 'Challenge.']) {
      if (!text.includes(want)) throw new Error(`the stat block has no "${want}": ${text}`);
    }
  });

  test('Edit on a loaded element in WHERE USED opens a copy of it in the pack', async (ctx) => {
    await newPack(ctx);
    await ctx.screen.getByRole('button', 'Spell Lists').tap();
    await expect(ctx.screen.getByRole('region', 'Spell lists')).toBeVisible();
    await ctx.browser.locator('.hb-lists select[aria-label="Spell list"]').selectOption('Wizard');
    const grid = '.hb-lists table';
    await waitFor(ctx, (g: string) => document.querySelector(g) !== null, grid, 'spell grid');
    await ctx.browser.evaluate((g: string) => {
      const b = [...document.querySelectorAll<HTMLButtonElement>(`${g} button.ap-name`)].find((x) => x.textContent === 'Fireball');
      b?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      b?.click();
      return b !== undefined;
    }, grid);
    await expect(ctx.screen.getByRole('region', 'Where used')).toBeVisible();
    await reveal(ctx, '.hb-where-list button[aria-label="Edit Wizard"]');
    await ctx.screen.getByRole('button', 'Edit Wizard').tap();
    await waitValue(ctx, NAME, 'Wizard (copy)');
    await expect(ctx.screen.getByRole('region', 'Element editor')).toBeVisible();
  });
});
