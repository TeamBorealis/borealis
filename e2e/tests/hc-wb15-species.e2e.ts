import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { statusMsg, waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-15 (spec 4.4, 13.4, 18.4 flow 4 on the web mock): the species forms with the creator switched on
// (`?creator=1`, desktop 1355x847), in a pack of both editions. Create a 2024 species with a 35-foot speed, darkvision
// and a size choice, save it and open it again; then a 2014 race and a subrace whose parent race is that race, with an
// ability score increase. Deterministic: no model.

const NAME = '#hb-f-name-input';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

async function newElement(ctx: Ctx, card: RegExp, name: string): Promise<void> {
  // the empty editor's New element, or the element list's New once the pack has an element
  await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
  await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
  await ctx.screen.getByRole('radio', card).tap();
  await ctx.browser.locator('#hb-new-name').fill(name);
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, name);
}

/** The number field `key` (its spin box), set by typing. */
async function setNumber(ctx: Ctx, key: string, value: number): Promise<void> {
  const box = `[data-field="${key}"] input[role="spinbutton"]`;
  await ctx.browser.locator(box).fill(String(value));
  await waitValue(ctx, box, String(value));
}

const sectionTitles = (ctx: Ctx): Promise<string[]> =>
  ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-editor .hb-section-title')].map((h) => h.textContent ?? ''), null);

const kindCount = (ctx: Ctx, text: RegExp, what: string): Promise<void> =>
  waitFor(ctx, (re: string) => new RegExp(re).test(document.querySelector('[aria-label="Element kinds"]')?.textContent ?? ''), text.source, what);

describe('hc-wb15 homebrew species forms (web mock)', { tags: ['homebrew'] }, () => {
  test('create a 2024 species with speed and darkvision, then a 2014 race and its subrace', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.browser.locator('#hb-pack-edition').selectOption('Both editions');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();

    // The species form: its sections; a new species is a Medium humanoid walking 30 feet.
    await newElement(ctx, /^Species/, 'Emberkin');
    expectEqual('species sections', 'Species,Speed,Senses,Traits,Details', (await sectionTitles(ctx)).join(','));
    await waitValue(ctx, '[data-field="speed"] input[role="spinbutton"]', '30');
    await waitValue(ctx, '#hb-f-creatureType-input', 'Humanoid');
    await expect(ctx.screen.getByRole('button', 'Medium')).toHaveAttribute('aria-pressed', 'true');

    await setNumber(ctx, 'speed', 35);
    await setNumber(ctx, 'darkvision', 120);
    await ctx.screen.getByRole('button', 'Small').tap();
    await expect(ctx.screen.getByRole('button', 'Small')).toHaveAttribute('aria-pressed', 'true');
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await autosaved(ctx);

    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Emberkin')).toBeVisible();
    expectEqual('status after save', "Saved 'Emberkin' to Ember Isles.", await statusMsg(ctx));
    await kindCount(ctx, /Species \(1\)/, 'Species (1)');

    // A 2014 race, then a subrace of it: the parent race is required, and the subrace has no size of its own.
    await newElement(ctx, /^Race A 2014/, 'Stonefolk');
    expectEqual('race sections', 'Race,Ability score increases,Speed,Senses,Languages,Traits,Random names,Details', (await sectionTitles(ctx)).join(','));
    await setNumber(ctx, 'constitution', 2);
    await setNumber(ctx, 'speed', 25);
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await autosaved(ctx);
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Stonefolk')).toBeVisible();

    await newElement(ctx, /^Subrace/, 'Ash Stonefolk');
    expectEqual('subrace sections', 'Subrace,Ability score increases,Speed,Senses,Languages,Traits,Details', (await sectionTitles(ctx)).join(','));
    await expect(ctx.screen.getByRole('button', /Problems \(1\)/)).toBeVisible();
    await ctx.browser.locator('#hb-f-parent-input').fill('Stonefolk');
    await setNumber(ctx, 'charisma', 1);
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await autosaved(ctx);
    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Ash Stonefolk')).toBeVisible();
    await kindCount(ctx, /Subraces \(1\)/, 'Subraces (1)');

    // The species opens again with its fields (CATEGORIES shows the species again).
    await ctx.screen.getByRole('button', 'Species (1)').tap();
    await ctx.screen.getByRole('button', 'Emberkin').tap();
    await waitValue(ctx, NAME, 'Emberkin');
    await waitValue(ctx, '[data-field="speed"] input[role="spinbutton"]', '35');
    await waitValue(ctx, '[data-field="darkvision"] input[role="spinbutton"]', '120');
    await expect(ctx.screen.getByRole('button', 'Small')).toHaveAttribute('aria-pressed', 'true');
  });
});
