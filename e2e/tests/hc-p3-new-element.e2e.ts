import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { type Ctx } from '../support/flows-a.js';
import { kindCount, newPack, waitValue, NAME, LOAD_MS } from '../support/creator.js';

// Board item H10MQ (HC11, desktop 1355x847, web mock): New element lists 37 kind cards, so picking one far down (Weapon)
// left the Name box scrolled away and the chosen card only faintly tinted; the new weapon then read "Weapons (0)" and
// Edition "New" until its first save. Deterministic: no model.

interface Seen { focused: boolean; nameTop: number; nameBottom: number; viewTop: number; viewBottom: number; chosenBorder: string; otherBorder: string; mark: string }

const look = (ctx: Ctx): Promise<Seen> => ctx.browser.evaluate((_: null) => {
  const name = document.getElementById('hb-new-name')!;
  let scroller = name.parentElement;
  while (scroller && !/(auto|scroll)/.test(getComputedStyle(scroller).overflowY)) scroller = scroller.parentElement;
  const view = (scroller ?? document.documentElement).getBoundingClientRect();
  const box = name.getBoundingClientRect();
  const chosen = document.querySelector('.hb-kind-card[aria-checked="true"]')!;
  const other = document.querySelector('.hb-kind-card[aria-checked="false"]')!;
  return {
    focused: document.activeElement === name, nameTop: Math.round(box.top), nameBottom: Math.round(box.bottom),
    viewTop: Math.round(view.top), viewBottom: Math.round(view.bottom),
    chosenBorder: getComputedStyle(chosen).borderTopColor, otherBorder: getComputedStyle(other).borderTopColor,
    mark: getComputedStyle(chosen, '::after').content,
  };
}, null);

describe('hc-p3 New element: the name after a kind, counts and edition before the first save (web mock)', { tags: ['homebrew'] }, () => {
  test('picking Weapon far down the cards brings the Name box back; the new weapon counts and shows its edition', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await newPack(ctx, 'Ember Isles');
    await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
    await ctx.screen.getByRole('radio', /^Weapon A weapon/).tap();
    const seen = await look(ctx);
    expectEqual('the Name box has the focus', 'true', String(seen.focused));
    expectEqual('the Name box is in view', 'true', String(seen.nameTop >= seen.viewTop && seen.nameBottom <= seen.viewBottom));
    expectEqual('the chosen card has its own border', 'true', String(seen.chosenBorder !== seen.otherBorder));
    expectEqual('the chosen card has a check', 'true', String(seen.mark !== 'none' && seen.mark !== 'normal'));

    await ctx.browser.locator('#hb-new-name').fill('Twinblade');
    await ctx.screen.getByRole('button', 'Create').tap();
    await waitValue(ctx, NAME, 'Twinblade', LOAD_MS);
    await kindCount(ctx, /Weapons \(1\)/);
    const row = await ctx.browser.evaluate((_: null) => document.querySelector('[aria-label="Elements"] tbody tr')?.textContent ?? '', null);
    expectEqual('the unsaved row shows the pack edition', 'true', String(/Twinblade/.test(row) && /Pack/.test(row) && !/New/.test(row)));
  });
});
