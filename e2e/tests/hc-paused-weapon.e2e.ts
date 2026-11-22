import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual, settledBox } from '../support/index.js';
import { textOf, waitFor, type Ctx } from '../support/flows-a.js';

// The two paused 1.5.0 creator items, checked on a weapon in the web mock (`?creator=1`):
// 1. desktop 1355x847: the element's title band and the Undo / Try it / Save row stay on screen while the form scrolls;
// 2. the weapon's Properties picker (multi-select): desktop rows carry the Magic grid's check box (spec 5.1), the picks are
//    not listed again under the list, and the description pane describes the property; on a phone (390x844) a checked
//    row's mark is drawn checked (its ::after carries the check mask), with the element's name in the sticky head.
// Deterministic: no model.

const NAME = '#hb-f-name-input';
const PHONE = { width: 390, height: 844 } as const;

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

async function reveal(ctx: Ctx, css: string): Promise<void> {
  await ctx.browser.evaluate((sel: string) => {
    const el = document.querySelector(sel);
    el?.scrollIntoView({ block: 'center' });
    return el !== null;
  }, css);
  await settledBox(ctx.browser.locator(css).first());
}

async function newPack(ctx: Ctx, name: string): Promise<void> {
  await ctx.screen.getByRole('button', 'New pack').first().tap();
  await ctx.browser.locator('#hb-pack-name').fill(name);
  await ctx.screen.getByRole('button', 'Create').tap();
  await expect(ctx.screen.getByRole('heading', `Homebrew: ${name}`)).toBeAttached();
}

async function newWeapon(ctx: Ctx, button: string, name: string): Promise<void> {
  await ctx.screen.getByRole('button', button).first().tap();
  await ctx.browser.locator('#hb-new-name').fill(name);
  await ctx.screen.getByRole('radio', /^Weapon A weapon/).tap();
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, name);
}

async function openProperties(ctx: Ctx): Promise<void> {
  await reveal(ctx, '[data-field="properties"] .hb-pick-btn');
  await ctx.browser.locator('[data-field="properties"] .hb-pick-btn').click();
  await expect(ctx.screen.getByRole('dialog', /^Choose/)).toBeVisible();
  await settledBox(ctx.browser.locator('.hb-pick-list .pick-row[data-row="Finesse"] button.pick-main').first());
}

describe('hc-paused creator weapon: fixed editor head and multi-select pickers (web mock)', { tags: ['homebrew'] }, () => {
  test('desktop: the title and commands stay while the form scrolls; property rows check with a box, once', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await newPack(ctx, 'Twin Forge');
    await newWeapon(ctx, 'New element', 'Twinblade');

    // The form scrolls inside its own panel; the title band and the command row do not move.
    const head = (_: null) => {
      const title = document.querySelector('.hb-editor > .ap-band')!.getBoundingClientRect();
      const save = document.querySelector('.hb-commands button[aria-label="Save"]')!.getBoundingClientRect();
      return `${Math.round(title.top)}:${Math.round(save.top)}`;
    };
    const before = String(await ctx.browser.evaluate(head, null));
    await ctx.browser.evaluate((_: null) => { const p = document.querySelector('.hb-tabpanel')!; p.scrollTop = p.scrollHeight; return true; }, null);
    await waitFor(ctx, (_: null) => document.querySelector('.hb-tabpanel')!.scrollTop > 200, null, 'the form scrolled');
    expectEqual('title band and Save where they were', before, String(await ctx.browser.evaluate(head, null)));
    expectEqual('the head names the weapon', 'Twinblade', await textOf(ctx, '.hb-editor > .ap-band h2'));
    await ctx.browser.evaluate((_: null) => { document.querySelector('.hb-tabpanel')!.scrollTop = 0; return true; }, null);

    // Properties: a check box per row checks it; the picks are not listed again; the pane describes the row.
    await openProperties(ctx);
    await ctx.browser.locator('.hb-pick-list .pick-row[data-row="Finesse"] input[type="checkbox"]').click();
    await ctx.browser.locator('.hb-pick-list .pick-row[data-row="Light"] input[type="checkbox"]').click();
    await ctx.browser.locator('.hb-pick-list .pick-row[data-row="Finesse"] button.pick-main').click();
    await waitFor(ctx, (_: null) => /Finesse/.test(document.querySelector('.hb-picker-desc')?.textContent ?? ''), null, 'Finesse described');
    const state = await ctx.browser.evaluate((_: null) => JSON.stringify({
      checked: [...document.querySelectorAll<HTMLInputElement>('.hb-pick-list .pick-row input[type="checkbox"]')].filter((c) => c.checked).map((c) => c.closest('li')!.getAttribute('data-row')),
      footer: document.querySelectorAll('.hb-picker .pick-footer').length,
      missing: (document.querySelector('.hb-picker-desc')?.textContent ?? '').includes('does not exist'),
    }), null);
    expectEqual('two checked rows, no footer list, the row described', JSON.stringify({ checked: ['Finesse', 'Light'], footer: 0, missing: false }), String(state));
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('dialog', /^Choose/)).toBeHidden();
    const names = await ctx.browser.evaluate((_: null) => [...document.querySelectorAll('[data-field="properties"] .hb-ref-name')].map((n) => n.textContent).join(','), null);
    expectEqual('the field lists both', 'Finesse,Light', String(names));
  });

  test('phone: the head names the weapon and a checked property row draws its check', async (ctx) => {
    await ctx.browser.setViewport(PHONE);
    await ctx.app.open('/?creator=1&mock=mobile#/homebrew');
    await expect(ctx.screen.getByRole('button', 'New pack').first()).toBeVisible();
    await newPack(ctx, 'Twin Forge');
    await newWeapon(ctx, 'New', 'Twinblade');
    expectEqual('the head names the weapon', 'Twinblade', await textOf(ctx, '.hb-editor-head h2'));

    await openProperties(ctx);
    await ctx.browser.locator('.hb-pick-list .pick-row[data-row="Finesse"] button.pick-main').click();
    const drawn = (row: string) => ctx.browser.evaluate((r: string) => {
      const mark = document.querySelector(`.hb-pick-list .pick-row[data-row="${r}"] .pick-mark`)!;
      const after = getComputedStyle(mark, '::after');
      const mask = after.maskImage || after.getPropertyValue('-webkit-mask-image');
      return `${mark.getAttribute('aria-checked')}:${mask !== '' && mask !== 'none'}`;
    }, row);
    expectEqual('Finesse checked and drawn checked', 'true:true', String(await drawn('Finesse')));
    expectEqual('Heavy clear and drawn clear', 'false:false', String(await drawn('Heavy')));
    expectEqual('no footer list', '0', String(await ctx.browser.evaluate((_: null) => document.querySelectorAll('.hb-picker .pick-footer').length, null)));
  });
});
