import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { AxeReport, LANDMARK_RULES } from '../support/axe.js';
import { clickCss, clickNth, loadSeraphine, waitFor } from '../support/flows-a.js';
import type { Ctx } from '../support/flows-a.js';

// IoeL0 (H-ajc follow-up, WB-26 section 17): axe's landmark and heading rules, moderate in axe and so outside the
// default check, run at every impact over the screens outside the creator (hc-next-landmarks.e2e.ts covers it): every
// main tab of an open character with each of its sub-tabs, the start screens (Additional Content, the Compendium), and
// the phone layout's tabs. Deterministic: the mock host, the seeded Seraphine Vale, no model.

const PHONE = { width: 390, height: 844 } as const;
/** A section's own sub-tabs: the band's tab strip (both layouts). */
const SUB = '.aur-subtabs [role=tab], .aur-subtabs nav a.tab';

/** The visible, enabled elements matching `css`, by their label (a phone tab's `.label`, else the text). */
function labels(ctx: Ctx, css: string): Promise<string[]> {
  return ctx.browser.evaluate((css: string) => Array.from(document.querySelectorAll<HTMLElement>(css))
    .filter((e) => e.offsetParent !== null && e.getAttribute('aria-disabled') !== 'true' && !e.closest('[role=dialog]'))
    .map((e) => (e.querySelector('.label')?.textContent ?? e.textContent ?? '').trim()), css);
}

/** How long a tab may take to be drawn and to take the selection after a click (a loaded machine is slow). */
const FIND_MS = 5000;

/** Index of the visible `css` element labelled `label`, brought into view; -1 when none is drawn. */
function find(ctx: Ctx, css: string, label: string): Promise<number> {
  // a phone's tab strip scrolls sideways: the tab is brought into view first
  return ctx.browser.evaluate(({ css, label }) => {
    const all = Array.from(document.querySelectorAll<HTMLElement>(css));
    const i = all.findIndex((e) => e.offsetParent !== null && !e.closest('[role=dialog]')
      && (e.querySelector('.label')?.textContent ?? e.textContent ?? '').trim() === label);
    all[i]?.scrollIntoView({ block: 'nearest', inline: 'center' });
    return i;
  }, { css, label });
}

/**
 * Click the visible `css` element labelled `label` (as {@link labels} reads it), then let the page settle. The element is
 * looked for until it is drawn (a page that just opened, the More page after a sheet closed), and a tab (`tab`) is waited
 * on until it is the selected one, so the sub-tabs read next are its own and not the last tab's.
 */
async function open(ctx: Ctx, css: string, label: string, tab = true): Promise<void> {
  const end = Date.now() + FIND_MS;
  let idx = await find(ctx, css, label);
  while (idx < 0 && Date.now() < end) {
    await new Promise((r) => setTimeout(r, 100));
    idx = await find(ctx, css, label);
  }
  if (idx < 0) throw new Error(`no ${css} "${label}"`);
  await clickNth(ctx, css, idx);
  if (tab) {
    await waitFor(ctx, ({ css, label }: { css: string; label: string }) => Array.from(document.querySelectorAll<HTMLElement>(css)).some((e) =>
      e.offsetParent !== null && (e.querySelector('.label')?.textContent ?? e.textContent ?? '').trim() === label
      && (e.getAttribute('aria-current') === 'page' || e.getAttribute('aria-selected') === 'true' || e.getAttribute('data-selected') === 'true')),
    { css, label }, `${label} selected`, FIND_MS);
  }
  await waitFor(ctx, (_: null) => document.readyState === 'complete', null, `${label} shown`);
}

/** The sweeps reach every tab (35 screens on desktop, 38 on the phone in this mock): fewer means a tab went unchecked. */
const MIN_SCREENS = 30;
const expectSweep = (screens: number): void => {
  if (screens < MIN_SCREENS) throw new Error(`axe checked only ${screens} screens (at least ${MIN_SCREENS} expected)`);
};

/** Every main tab, and each sub-tab of it, checked once. */
async function sweep(ctx: Ctx, axe: AxeReport, prefix: string, main: string, sub: string): Promise<void> {
  for (const tab of await labels(ctx, main)) {
    await open(ctx, main, tab);
    await axe.check(ctx, `${prefix}${tab}`);
    for (const s of await labels(ctx, sub)) {
      await open(ctx, sub, s);
      await axe.check(ctx, `${prefix}${tab} > ${s}`);
    }
  }
}

describe('landmarks and headings outside the creator (axe, web mock)', { tags: ['a11y'] }, () => {
  test('desktop: every tab of an open character, Additional Content and the Compendium', async (ctx) => {
    const axe = new AxeReport({ rules: LANDMARK_RULES });
    await loadSeraphine(ctx);
    await sweep(ctx, axe, '', 'nav[aria-label="Main"] a.tab', SUB);
    for (const [route, heading] of [['/#/content', 'Additional Content'], ['/#/compendium', 'Compendium']] as const) {
      await ctx.app.open(route);
      await expect(ctx.screen.getByRole('menuitem', 'File')).toBeVisible();
      await axe.check(ctx, heading);
    }
    expectSweep(axe.assertClean());
  });

  test('phone: every tab of an open character', async (ctx) => {
    const axe = new AxeReport({ rules: LANDMARK_RULES });
    await ctx.browser.setViewport(PHONE);
    await ctx.app.open('/?mock=mobile#/characters');
    await expect(ctx.screen.getByRole('heading', 'Characters')).toBeVisible();
    await axe.check(ctx, 'phone characters');
    await clickCss(ctx, '.v4-tile-open[aria-label="Seraphine Vale, Level 2 Elf Wizard"]');
    await clickCss(ctx, '.v4-tile-openbtn[aria-label="Open Seraphine Vale"]');
    await waitFor(ctx, (_: null) => !location.hash.startsWith('#/characters'), null, 'Seraphine open');
    await sweep(ctx, axe, 'phone ', 'nav.aur-bottombar a.bb-item', SUB);
    // the More page's destinations
    await open(ctx, 'nav.aur-bottombar a.bb-item', 'More');
    for (const dest of await labels(ctx, '[aria-label="More destinations"] a')) {
      await open(ctx, '[aria-label="More destinations"] a', dest, false);
      await axe.check(ctx, `phone More > ${dest}`);
      // a destination that opens as a sheet (the Compendium) closes with Escape
      await ctx.browser.keyboard.press('Escape');
      await open(ctx, 'nav.aur-bottombar a.bb-item', 'More');
    }
    expectSweep(axe.assertClean());
  });
});
