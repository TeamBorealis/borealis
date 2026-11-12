import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { waitFor, type Ctx } from '../support/flows-a.js';
import { newElement, newPack } from '../support/creator.js';

// Homebrew WB-28, board item H-aDY (decision HC10 Q3, U7: Aurora's DataGrid keeps its header): the class's LEVELS and
// SPELLCASTING grids keep their header row on screen while the form scrolls past the grid, at the top of the form's
// scroller on desktop (1355x847) and under the sticky editor head on a phone (390x844, where the spellcasting grid is a
// real grid with a header row, not the phone's one-line rows). Deterministic: no model.

const PHONE = { width: 390, height: 844 } as const;

interface Pin { head: number; view: number; display: string }

/** Scrolls the form so the grid's top is `past` px above the visible area, then reads where its header row is. */
const scrollPast = (ctx: Ctx, grid: string, past: number): Promise<Pin> => ctx.browser.evaluate(async ({ css, by }: { css: string; by: number }) => {
  const table = document.querySelector(css)!;
  let scroller = table.parentElement!.parentElement;
  while (scroller && !/(auto|scroll)/.test(getComputedStyle(scroller).overflowY)) scroller = scroller.parentElement;
  const head = document.querySelector('.hb-ws.narrow .hb-editor-head');
  const viewTop = () => Math.max(scroller!.getBoundingClientRect().top, head ? head.getBoundingClientRect().bottom : -Infinity);
  scroller!.scrollTop += table.getBoundingClientRect().top - viewTop() + by;
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  const th = table.querySelector('thead th')!;
  return { head: Math.round(th.getBoundingClientRect().top), view: Math.round(viewTop()), display: getComputedStyle(table.querySelector('thead')!).display };
}, { css: grid, by: past });

async function addSpellcasting(ctx: Ctx): Promise<void> {
  await ctx.screen.getByRole('tab', 'SPELLCASTING').tap();
  await ctx.screen.getByRole('button', 'Add spellcasting').tap();
  await waitFor(ctx, (_: null) => document.querySelector('.hb-caster-table') !== null, null, 'the spellcasting grid');
}

describe('hc-wb28 the class grids keep their header row on screen (web mock)', { tags: ['homebrew'] }, () => {
  test('desktop: LEVELS and SPELLCASTING headers stay at the top of the form', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await newPack(ctx, 'Ember Isles');
    await newElement(ctx, 'Ember Knight', /^Class A class/);

    await ctx.screen.getByRole('tab', 'LEVELS').tap();
    await waitFor(ctx, (_: null) => document.querySelector('.hb-levels:not(.hb-caster-table)') !== null, null, 'the LEVELS grid');
    const levels = await scrollPast(ctx, '.hb-levels:not(.hb-caster-table)', 200);
    expectEqual('LEVELS header at the top of the form', String(levels.view), String(levels.head));

    await addSpellcasting(ctx);
    const caster = await scrollPast(ctx, '.hb-caster-table', 150);
    expectEqual('SPELLCASTING header at the top of the form', String(caster.view), String(caster.head));
    // scrolled back above the grid, the header sits on the grid again
    const back = await scrollPast(ctx, '.hb-caster-table', -100);
    expectEqual('header back on its grid', String(back.view + 100), String(back.head));
  });

  test('phone: the SPELLCASTING grid has a header row that stays under the editor head', async (ctx) => {
    await ctx.browser.setViewport(PHONE);
    await ctx.app.open('/?creator=1&mock=mobile#/homebrew');
    await newPack(ctx, 'Ember Isles');
    await newElement(ctx, 'Ember Knight', /^Class A class/);
    await expect(ctx.screen.getByRole('tab', 'SPELLCASTING')).toBeVisible();

    await addSpellcasting(ctx);
    const caster = await scrollPast(ctx, '.hb-caster-table', 200);
    expectEqual('a header row', 'table-header-group', caster.display);
    expectEqual('SPELLCASTING header under the editor head', String(caster.view), String(caster.head));
  });
});
