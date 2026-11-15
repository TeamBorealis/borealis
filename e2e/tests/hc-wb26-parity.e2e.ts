import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { waitFor, type Ctx } from '../support/flows-a.js';

// Homebrew WB-26 (U7 parity audit 11, spec 13.2-13.4) on the web mock at 1355x847 with the creator switched on: the
// workspace keeps Aurora's Equipment gutters (CATEGORIES at x51, the editor ending at the grid's x920, the description
// pane from x940); every sub-tab of a class (nine) is on screen in the band, none under the Expert switch, which sits in
// the command row; the description toolbar is one row; the SPELLCASTING grid's count headings are whole; enabled
// commands read #C4C4C4 (Aurora's inventory panel), disabled ones #6D6D6D. Deterministic: no model.

const NAME = '#hb-f-name-input';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

interface Box { x: number; y: number; r: number; b: number }

const box = (ctx: Ctx, css: string): Promise<Box | null> => ctx.browser.evaluate((sel: string) => {
  const r = document.querySelector(sel)?.getBoundingClientRect();
  return r ? { x: Math.round(r.left), y: Math.round(r.top), r: Math.round(r.right), b: Math.round(r.bottom) } : null;
}, css);

async function newElement(ctx: Ctx, kind: RegExp, name: string): Promise<void> {
  await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
  await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
  await ctx.screen.getByRole('radio', kind).tap();
  await ctx.browser.locator('#hb-new-name').fill(name);
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, name);
}

describe('hc-wb26 creator parity at 1355x847 (web mock)', { tags: ['homebrew'] }, () => {
  test('gutters, the class sub-tab band, the toolbar row, the spellcasting grid and command colours', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();
    await newElement(ctx, /^Class A class/, 'Ember Knight');

    // Aurora Equipment: CATEGORIES 51, grid 230 (+-1), grid ends at 920, splitter and pane from 940
    expectEqual('categories x', 51, String((await box(ctx, '.hb-cats'))?.x));
    expectEqual('editor right edge', 920, String((await box(ctx, '.hb-ws-editor'))?.r));
    expectEqual('description pane x', 940, String((await box(ctx, '.hb-ws-desc'))?.x));

    // Every tab inside the band and on screen; the Expert switch is in the command row, not over a tab
    const tabs = await ctx.browser.evaluate((_: null) => {
      const band = document.querySelector('.hb-band')!.getBoundingClientRect();
      return [...document.querySelectorAll('.hb-band [role="tab"]')].map((t) => {
        const r = t.getBoundingClientRect();
        return `${t.textContent}:${r.left >= band.left && r.right <= band.right + 0.5 && r.top >= band.top && r.bottom <= band.bottom}`;
      });
    }, null);
    expectEqual('all nine tabs inside the band', 'GENERAL:true,PROFICIENCIES:true,LEVELS:true,SPELLCASTING:true,MULTICLASS:true,EQUIPMENT:true,RULES:true,DESCRIPTION:true,DATA:true', tabs.join(','));
    expectEqual('switch in the command row', 'true', String(await ctx.browser.evaluate((_: null) => !!document.querySelector('.hb-cmdrow [role="switch"]') && !document.querySelector('.hb-band [role="switch"]'), null)));

    // Command colours: Save enabled, Redo disabled
    const colours = await ctx.browser.evaluate((_: null) => ['Save', 'Redo'].map((l) => getComputedStyle(document.querySelector(`.hb-commands button[aria-label="${l}"]`)!).color), null);
    expectEqual('enabled / disabled command colours', 'rgb(196, 196, 196),rgb(109, 109, 109)', colours.join(','));

    // DESCRIPTION: the toolbar keeps one row
    await ctx.screen.getByRole('tab', 'DESCRIPTION').tap();
    await expect(ctx.screen.getByRole('toolbar', 'Description formatting')).toBeVisible();
    const rows = await ctx.browser.evaluate((_: null) => new Set([...document.querySelectorAll('.rich-toolbar button')].map((b) => Math.round(b.getBoundingClientRect().top))).size, null);
    expectEqual('toolbar rows', 1, rows);

    // SPELLCASTING: the count headings are whole, the nine slot levels fit
    await ctx.screen.getByRole('tab', 'SPELLCASTING').tap();
    await ctx.screen.getByRole('button', 'Add spellcasting').tap();
    await expect(ctx.screen.getByRole('table', 'Spell slots and spells by level')).toBeAttached();
    const heads = await ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-caster-table thead th')]
      .map((t) => `${t.textContent}:${t.scrollWidth <= t.clientWidth}`), null);
    expectEqual('grid headings whole', 'Level:true,Cantrips:true,Prepared:true,1st:true,2nd:true,3rd:true,4th:true,5th:true,6th:true,7th:true,8th:true,9th:true', heads.join(','));
    const grid = await box(ctx, '.hb-caster-table');
    const frame = await box(ctx, '.hb-levels-wrap');
    expectEqual('grid fits its frame', 'true', String(!!grid && !!frame && grid.r <= frame.r));
  });
});
