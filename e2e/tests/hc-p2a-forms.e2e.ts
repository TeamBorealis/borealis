import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew P2a (1.5.0, the web mock, desktop 1355x847): children made inside children and a class option group made in
// one flow. A species' racial trait made inside the species and the trait's lineage option made inside the trait; a 2014
// background variant's own feature made inside the variant; a class's "Lets the player choose" block of class options
// making its group and an option. Each is saved and (the first two) opened again. Deterministic: no model.

const NAME = '#hb-f-name-input';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

const kindCount = (ctx: Ctx, text: RegExp, what: string): Promise<void> =>
  waitFor(ctx, (re: string) => new RegExp(re).test(document.querySelector('[aria-label="Element kinds"]')?.textContent ?? ''), text.source, what);

async function newPack(ctx: Ctx, edition: string): Promise<void> {
  await bootToHome(ctx, '/?creator=1#/homebrew');
  await ctx.screen.getByRole('button', 'New pack').first().tap();
  await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
  await ctx.browser.locator('#hb-pack-edition').selectOption(edition);
  await ctx.screen.getByRole('button', 'Create').tap();
  await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();
}

async function newElement(ctx: Ctx, card: RegExp, name: string): Promise<void> {
  await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
  await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
  await ctx.screen.getByRole('radio', card).tap();
  await ctx.browser.locator('#hb-new-name').fill(name);
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, name);
}

/** Done in the child dialog of the form `key` (its body carries `data-child`; not the dialog of a child inside it). */
async function done(ctx: Ctx, key: string): Promise<void> {
  await ctx.browser.evaluate((k: string) => {
    const own = [...document.querySelectorAll('[role="dialog"]')].find((d) => d.querySelector(`[data-child="${k}"]`)
      && [...d.querySelectorAll('[data-child]')].every((x) => x.getAttribute('data-child') === k));
    const button = [...(own?.querySelectorAll('button') ?? [])].find((b) => b.textContent === 'Done') as HTMLButtonElement | undefined;
    button?.click();
    return button !== undefined;
  }, key);
  await waitFor(ctx, (k: string) => document.querySelector(`[data-child="${k}"]`) === null, key, `the dialog of ${key} closed`);
}

/** Name the child `key` in its dialog. */
async function nameChild(ctx: Ctx, key: string, name: string): Promise<void> {
  const box = `#hb-f-c-${key}-name-input`;
  await ctx.browser.locator(box).fill(name);
  await waitValue(ctx, box, name);
}

describe('hc-p2a homebrew children inside children and class option groups (web mock)', { tags: ['homebrew'] }, () => {
  test('a species: a racial trait made inside it, and a lineage option made inside the trait; saved and opened again', async (ctx) => {
    await newPack(ctx, 'Both editions');
    await newElement(ctx, /^Species/, 'Emberkin');

    await ctx.screen.getByRole('button', 'Add a racial trait').tap();
    await expect(ctx.screen.getByRole('dialog', /^Racial trait:/)).toBeVisible();
    await nameChild(ctx, 'n1', 'Ember Lineage');
    await ctx.screen.getByRole('button', 'Add a species trait option').tap();
    await expect(ctx.screen.getByRole('dialog', /^Species trait option:/)).toBeVisible();
    await nameChild(ctx, 'n2', 'Ash');
    await done(ctx, 'n2');
    await expect(ctx.screen.getByRole('button', 'Ash')).toBeVisible();
    await done(ctx, 'n1');
    await expect(ctx.screen.getByRole('button', 'Ember Lineage')).toBeVisible();
    await autosaved(ctx);

    await pressKey(ctx, 'Control+s');
    await kindCount(ctx, /Species \(1\)/, 'Species (1)');
    await ctx.screen.getByRole('button', 'Emberkin').tap();
    await waitValue(ctx, NAME, 'Emberkin');
    await ctx.screen.getByRole('button', 'Ember Lineage').tap();
    await expect(ctx.screen.getByRole('dialog', 'Racial trait: Ember Lineage')).toBeVisible();
    await expect(ctx.screen.getByRole('group', 'Lineage options')).toContainText('Ash');
  });

  test('a 2014 background: a variant with its own feature made inside it; saved and opened again', async (ctx) => {
    await newPack(ctx, '2014');
    await newElement(ctx, /^Background A background/, 'Wanderer');

    await ctx.screen.getByRole('button', 'Add a background variant').tap();
    await expect(ctx.screen.getByRole('dialog', /^Background variant:/)).toBeVisible();
    await nameChild(ctx, 'n1', 'Night Walker');
    // the variant's own feature (the background's own Feature list under the dialog has the same button)
    await ctx.browser.evaluate((_: null) => {
      const b = document.querySelector<HTMLButtonElement>('[data-child="n1"] [data-field="feature"] button.hb-pick-btn');
      b?.click();
      return b !== null;
    }, null);
    await expect(ctx.screen.getByRole('dialog', /^Background feature:/)).toBeVisible();
    await nameChild(ctx, 'n2', 'Night Eyes');
    await done(ctx, 'n2');
    await waitFor(ctx, (_: null) => /Night Eyes/.test(document.querySelector('[data-child="n1"] [data-field="feature"]')?.textContent ?? ''), null, 'the variant lists its feature');
    await done(ctx, 'n1');
    await expect(ctx.screen.getByRole('group', 'Variants')).toContainText('Night Walker');
    await autosaved(ctx);

    await pressKey(ctx, 'Control+s');
    await kindCount(ctx, /Backgrounds \(1\)/, 'Backgrounds (1)');
    await ctx.screen.getByRole('button', 'Wanderer').tap();
    await waitValue(ctx, NAME, 'Wanderer');
    await ctx.screen.getByRole('button', 'Night Walker').tap();
    await expect(ctx.screen.getByRole('dialog', 'Background variant: Night Walker')).toBeVisible();
    await expect(ctx.screen.getByRole('group', 'Its own feature')).toContainText('Night Eyes');
  });

  test('a class: a choice of class options makes its group and an option in one flow', async (ctx) => {
    await newPack(ctx, '2024');
    await newElement(ctx, /^Class A class/, 'Ember Knight');
    await ctx.browser.locator('#hb-f-hitDie-input').selectOption('d10');

    await ctx.screen.getByRole('tab', 'RULES').tap();
    await ctx.screen.getByRole('button', 'Add rule').tap();
    await expect(ctx.screen.getByRole('dialog', 'Add a rule')).toBeVisible();
    await ctx.screen.getByRole('button', 'Lets the player choose ...').tap();
    await expect(ctx.screen.getByRole('dialog', 'New rule: Lets the player choose ...')).toBeVisible();
    await ctx.browser.locator('.hb-block-dialog [data-field="type"] input').fill('Option');
    await expect(ctx.screen.getByRole('region', 'Class options of this choice')).toBeVisible();
    await ctx.browser.locator('[data-field="optionGroup"] input').fill('Ember Style');
    await waitValue(ctx, '[data-field="optionGroup"] input', 'Ember Style');
    await ctx.browser.locator('[data-field="newOption"] input').fill('Ash Guard');
    await ctx.screen.getByRole('button', 'Add option').tap();
    await expect(ctx.screen.getByRole('list', 'Options of Ember Style')).toContainText('Ash Guard');
    await ctx.screen.getByRole('button', 'OK').tap();
    await waitFor(ctx, (_: null) => /Ember Style/.test(document.querySelector('.hb-block-list')?.textContent ?? ''), null, 'the choice block');
    await kindCount(ctx, /Class options \(1\)/, 'Class options (1)');
    await autosaved(ctx);

    await pressKey(ctx, 'Control+s');
    await kindCount(ctx, /Classes \(1\)/, 'Classes (1)');
  });
});
