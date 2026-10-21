import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, cssOf, edgesOf, expectLayout, realClick, rgb, settledEdgesOf, TEXT_TOLERANCE_PX, textLeftOf } from '../support/index.js';
import { clickByText, clickMainTab, loadSeraphine, waitFor } from '../support/flows-a.js';
import { pressKey, textsOf } from '../support/flows-b.js';
import type { BrowserLike, LocatorLike, ScreenLike } from '../support/types.js';

// U7 (UI matches the classic builder "down to a dotted i"): per-screen geometry, colours and structure.
// Reference measurements at the fixed 1355x847 window, reference value first. Deterministic: MockApi, no model.

const LINE = '#252d31';
const ACCENT = '#293338';
const FIXED_CATEGORY_ORDER = [
  'Adventuring Gear', 'Treasure', 'Trade Goods', 'Equipment Packs', 'Tools', 'Musical Instruments', 'Armor', 'Magic Armor',
  'Weapons', 'Magic Weapons', 'Ammunition', 'Spellcasting Focus', 'Wondrous Items', 'Supernatural Gifts', 'Staffs', 'Rods',
  'Wands', 'Rings', 'Potions', 'Poison', 'Scrolls', 'Spell Scrolls',
];

type Ctx = Parameters<typeof bootToHome>[0] & Parameters<typeof edgesOf>[0];
const goTo = (ctx: Ctx, hash: string) => ctx.browser.evaluate((h: string) => { location.hash = h; return 1; }, hash);

interface SettingsCtx {
  browser: BrowserLike & { locator(css: string): LocatorLike };
  screen: ScreenLike & { getByRole(role: string, name: string | RegExp): { tap(): Promise<void> } };
}

async function openSettingsTab(ctx: SettingsCtx, tabId: string): Promise<void> {
  await ctx.screen.getByRole('menuitem', 'View').tap();
  await ctx.screen.getByRole('menuitem', /^Application settings/i).tap();
  await expect(ctx.screen.getByRole('dialog', 'Application Settings') as never).toBeVisible();
  await realClick(ctx, ctx.browser.locator(`#${tabId}`), `#${tabId}`);
}

describe('u7 screens', { tags: ['u7'] }, () => {
  test('New character flyout: field column x975, gold standard-array tiles, 26 px footer buttons', async (ctx) => {
    const { browser, screen } = ctx;
    await bootToHome(ctx);
    await realClick(ctx, browser.locator('button.aur-newchar'), 'button.aur-newchar');
    await expect(screen.getByRole('dialog', 'New character')).toBeVisible();
    await settledEdgesOf(ctx, '.flyout');
    expectLayout('first field label x', 975, (await edgesOf(ctx, '.flyout label')).left);
    const label = await cssOf(ctx, '.flyout label', ['font-size', 'font-weight', 'color', 'text-transform']);
    expectLayout('field label size', '10px', label['font-size']!);
    expectLayout('field label weight', '600', label['font-weight']!);
    expectLayout('field label colour', rgb('#7f7f7f'), label.color!);
    expectLayout('field label case', 'uppercase', label['text-transform']!);
    await browser.locator('#v4-method').selectOption({ value: 'standard_array' });
    await waitFor(ctx, (_: null) => document.querySelectorAll('.v4-tile-abil:not([disabled])').length === 6, null, 'six enabled tiles');
    const tile = await edgesOf(ctx, '.v4-tile-abil');
    expectLayout('first tile left', 975, tile.left);
    expectLayout('first tile right', 1021, tile.right);
    expectLayout('tile value colour', rgb('#b38000'), (await cssOf(ctx, '.v4-tile-abil span', ['color'])).color!);
    expectLayout('tile border', rgb(ACCENT), (await cssOf(ctx, '.v4-tile-abil', ['border-top-color']))['border-top-color']!);
    const create = await edgesOf(ctx, '.flyout .v4-foot button', 0);
    const cancel = await edgesOf(ctx, '.flyout .v4-foot button', 1);
    expectLayout('footer button height', 26, create.height);
    expectLayout('footer buttons share a row', create.top, cancel.top, 0);
    expectLayout('footer buttons are equal width', create.width, cancel.width);
    expectLayout('footer first button x', 975, create.left);
  });

  test('Build picker: 26 px accent expander header at y141 with the dark rule', async (ctx) => {
    await loadSeraphine(ctx);
    const head = await edgesOf(ctx, '.pick .aur-expander-head');
    expectLayout('picker header top', 141, head.top);
    expectLayout('picker header bottom', 166, head.bottom);
    const css = await cssOf(ctx, '.pick .aur-expander-head', ['background-color', 'border-top-color']);
    expectLayout('picker header fill', rgb(ACCENT), css['background-color']!);
    expectLayout('picker header rule', rgb(LINE), css['border-top-color']!);
    expectLayout('build tabs', 'Species|Class|Background|Ability Scores|Languages|Proficiencies|Feats', (await textsOf(ctx, '[role=tab]')).join('|'));
  });

  test('Equipment: category pane edge 219, grid border 230, NAME ink 236, row lines y202 then every 21 px', async (ctx) => {
    await loadSeraphine(ctx);
    await clickMainTab(ctx, 'Equipment');
    await waitFor(ctx, (_: null) => document.querySelectorAll('.eq-grid tbody tr').length > 0, null, 'equipment catalogue');
    const cats = (await textsOf(ctx, '.eq-cats button')).map((t) => t.replace(/\s*\(\d+\)$/, ''));
    const fixed = cats.filter((c) => FIXED_CATEGORY_ORDER.includes(c));
    expectLayout('fixed categories in reference order', fixed.join('|'), [...fixed].sort((a, b) => FIXED_CATEGORY_ORDER.indexOf(a) - FIXED_CATEGORY_ORDER.indexOf(b)).join('|'));
    expectLayout('fixed categories lead the list', fixed.join('|'), cats.slice(0, fixed.length).join('|'));
    const firstAdditional = cats.findIndex((c) => c.startsWith('Additional '));
    if (firstAdditional >= 0 && cats.slice(firstAdditional).some((c) => !c.startsWith('Additional '))) {
      throw new Error(`U7 generated "Additional ..." categories must come last: ${cats.join('|')}`);
    }
    await clickByText(ctx, 'button', 'Armor (');
    await waitFor(ctx, (_: null) => Array.from(document.querySelectorAll('.eq-grid tbody tr')).some((r) => (r.textContent ?? '').includes('Chain Mail')), null, 'armor rows');
    expectLayout('category pane right edge', 219, (await edgesOf(ctx, '.eq-cats')).right);
    expectLayout('item grid border x', 230, (await edgesOf(ctx, '.eq-items')).left);
    for (const css of ['.eq-cats', '.eq-items']) {
      expectLayout(`${css} border colour`, rgb(LINE), (await cssOf(ctx, css, ['border-left-color']))['border-left-color']!);
    }
    expectLayout('NAME header text x', 236, await textLeftOf(ctx, '.eq-grid.eq-armor thead th'), TEXT_TOLERANCE_PX);
    expectLayout('first cell text x', 236, await textLeftOf(ctx, '.eq-grid.eq-armor tbody td'), TEXT_TOLERANCE_PX);
    const th = await cssOf(ctx, '.eq-grid.eq-armor thead th', ['font-size', 'font-weight', 'text-transform']);
    expectLayout('grid header size', '9px', th['font-size']!);
    expectLayout('grid header weight', '600', th['font-weight']!);
    expectLayout('grid header case', 'uppercase', th['text-transform']!);
    expectLayout('grid band fill', rgb(ACCENT), (await cssOf(ctx, '.eq-items .ap-band', ['background-color']))['background-color']!);
    const rows = await ctx.browser.evaluate((_: null) => Array.from(document.querySelectorAll('.eq-grid.eq-armor tbody tr')).slice(0, 3).map((r) => Math.round(r.getBoundingClientRect().bottom)), null);
    expectLayout('first row line y', 202, rows[0]!, 0);
    expectLayout('row pitch', 21, rows[1]! - rows[0]!, 0);
    expectLayout('row pitch (next)', 21, rows[2]! - rows[1]!, 0);
    expectLayout('row line colour', rgb('#141414'), (await cssOf(ctx, '.eq-grid tbody tr td', ['border-bottom-color']))['border-bottom-color']!);
  });

  test('Manage and Character Sheet: tab order and 26 px command buttons on the x51 column', async (ctx) => {
    await loadSeraphine(ctx);
    await clickMainTab(ctx, 'Manage');
    await waitFor(ctx, (_: null) => location.hash.startsWith('#/manage'), null, 'manage route');
    expectLayout('manage tabs', 'Character|Backstory|Notes|Allies & Organizations|Attacks & Spellcasting', (await textsOf(ctx, '[role=tab]')).join('|'));
    await clickMainTab(ctx, 'Character Sheet');
    await waitFor(ctx, (_: null) => document.querySelectorAll('.sh-btn').length >= 2, null, 'sheet buttons');
    const first = await edgesOf(ctx, '.sh-btn', 0);
    const second = await edgesOf(ctx, '.sh-btn', 1);
    expectLayout('sheet button x', 51, first.left);
    expectLayout('sheet button height', 26, first.height);
    expectLayout('sheet buttons share a row', first.top, second.top, 0);
  });

  test('Compendium: bottom flyout 98 px under the client top, full width, Monster group and stat facts', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);
    await screen.getByLabel('Compendium search').fill('goblin');
    await pressKey(ctx, 'Enter');
    await expect(screen.getByRole('dialog', 'Compendium')).toBeVisible();
    const panel = await settledEdgesOf(ctx, '.flyout');
    expectLayout('compendium top (window border 1 + margin 98)', 99, panel.top);
    expectLayout('compendium last row', 818, panel.bottom);
    expectLayout('compendium left', 1, panel.left);
    expectLayout('compendium width', 1353, panel.width);
    const frame = await cssOf(ctx, '.flyout', ['border-top-width', 'border-top-color']);
    expectLayout('compendium top border', '2px', frame['border-top-width']!);
    expectLayout('compendium top border colour', rgb('#29333866'), frame['border-top-color']!);
    const types = await browser.evaluate((_: null) => Array.from((document.querySelector('[aria-label="Filter by type"]') as HTMLSelectElement).options).map((o) => o.textContent ?? ''), null);
    if (!types.some((t) => /^Monster \(\d+\)$/.test(t))) throw new Error(`U7 type filter has no Monster group: ${types.join('|')}`);
    expectLayout('monster result group', 'Monster', (await textsOf(ctx, '.v4-comp .aur-expander-title')).find((t) => /monster/i.test(t)) ?? '(none)');
    await realClick(ctx, browser.locator('.v4-el button[aria-selected]').first(), '.v4-el button[aria-selected]');
    await expect(browser.locator('.v4-desc article')).toBeVisible();
    expectLayout('monster facts', 'Type|Armor Class|Hit Points|Challenge', (await textsOf(ctx, '.v4-desc dl.kv dt')).join('|'));
    expectLayout('monster type fact', 'Monster', (await textsOf(ctx, '.v4-desc dl.kv dd'))[0] ?? '');
  });

  test('Additional content: section heads, inline notice link that keeps the copy line, INSTALLED PACKS at y208', async (ctx) => {
    await bootToHome(ctx);
    await goTo(ctx, '#/content');
    await waitFor(ctx, (_: null) => document.querySelectorAll('.aur-content h2').length >= 2, null, 'content sections');
    const heads = await textsOf(ctx, '.aur-content h2');
    expectLayout('first two sections', 'Custom content|Installed packs', heads.slice(0, 2).join('|'));
    const h2 = await cssOf(ctx, '.aur-content h2', ['font-size', 'font-weight', 'color']);
    expectLayout('section head size', '13px', h2['font-size']!);
    expectLayout('section head weight', '600', h2['font-weight']!);
    expectLayout('section head colour', rgb('#e6e6e6'), h2.color!);
    expectLayout('section head x', 51, (await edgesOf(ctx, '.aur-content h2')).left);
    expectLayout('INSTALLED PACKS head top', 208, (await edgesOf(ctx, '.aur-content h2', 1)).top);
    const link = await cssOf(ctx, '.v4-inline-link', ['font-size', 'color', 'text-transform']);
    expectLayout('notice link size', '10px', link['font-size']!);
    expectLayout('notice link colour', rgb('#b38000'), link.color!);
    expectLayout('notice link case', 'uppercase', link['text-transform']!);
    // A text link, not a 26 px command box: the copy line keeps its height.
    const linkBox = await edgesOf(ctx, '.v4-inline-link');
    const copyLine = await edgesOf(ctx, '.aur-content h2 + * p, .aur-content p.muted');
    if (linkBox.height >= 20) throw new Error(`U7 notice link is a ${linkBox.height}px box; it must stay a text-height link`);
    if (linkBox.bottom > copyLine.bottom) throw new Error(`U7 notice link overhangs its copy line (${linkBox.bottom} > ${copyLine.bottom})`);
  });

  test('Application Settings: 500x600 centred window, title bar 30, footer #101010, DATA FILES section heads', async (ctx) => {
    await bootToHome(ctx);
    await openSettingsTab(ctx, 'settings-data');
    const win = await edgesOf(ctx, '.aur-set-win');
    expectLayout('settings width', 500, win.width);
    expectLayout('settings height', 600, win.height);
    expectLayout('settings left (centred)', 428, win.left);
    expectLayout('settings top (centred)', 124, win.top);
    expectLayout('settings border', rgb(ACCENT), (await cssOf(ctx, '.aur-set-win', ['border-top-color']))['border-top-color']!);
    const head = await edgesOf(ctx, '.aur-set-head');
    expectLayout('settings title bar height', 30, head.height);
    expectLayout('settings title bar fill', rgb('#21292d'), (await cssOf(ctx, '.aur-set-head', ['background-color']))['background-color']!);
    expectLayout('settings footer fill', rgb('#101010'), (await cssOf(ctx, '.aur-set-foot', ['background-color']))['background-color']!);
    expectLayout('data files section heads', 'DATA FILES|ADDITIONAL CONTENT|LOGS', (await textsOf(ctx, '.aur-set-win h3')).join('|'));
    const h3 = await cssOf(ctx, '.aur-set-win h3', ['font-size', 'font-weight', 'color']);
    expectLayout('section head size', '13px', h3['font-size']!);
    expectLayout('section head weight', '600', h3['font-weight']!);
    expectLayout('section head colour', rgb('#d5d5d5'), h3.color!);
    const logsSize = await ctx.browser.evaluate((_: null) => {
      const logs = Array.from(document.querySelectorAll('.aur-set-win h3')).find((h) => (h.textContent ?? '').trim().toUpperCase() === 'LOGS');
      return logs ? getComputedStyle(logs).fontSize : 'missing';
    }, null);
    expectLayout('LOGS is a section head', '13px', logsSize);
  });

  test('dialogs: title bar 30 #21292d, footer #101010 with 10 px pad, flat #242425 buttons', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);
    await goTo(ctx, '#/content');
    await browser.locator('input[aria-label="Choose content files"]').setInputFiles('fixtures/import/quick-feats.xml');
    await expect(screen.getByRole('dialog', /Import only content you own/)).toBeVisible();
    const head = await edgesOf(ctx, '[role=dialog] .dialog-head');
    expectLayout('dialog title bar height', 30, head.height);
    expectLayout('dialog title bar fill', rgb('#21292d'), (await cssOf(ctx, '[role=dialog] .dialog-head', ['background-color']))['background-color']!);
    expectLayout('dialog border', rgb(ACCENT), (await cssOf(ctx, '[role=dialog]', ['border-top-color']))['border-top-color']!);
    const foot = await edgesOf(ctx, '[role=dialog] .dialog-foot');
    expectLayout('dialog footer fill', rgb('#101010'), (await cssOf(ctx, '[role=dialog] .dialog-foot', ['background-color']))['background-color']!);
    const last = await edgesOf(ctx, '[role=dialog] .dialog-foot button:last-child');
    expectLayout('dialog footer right pad', 10, foot.right - last.right);
    expectLayout('dialog button height', 26, last.height);
    for (const css of ['[role=dialog] .dialog-foot button:first-child', '[role=dialog] .dialog-foot button:last-child']) {
      expectLayout(`${css} fill`, rgb('#242425'), (await cssOf(ctx, css, ['background-color']))['background-color']!);
    }
  });
});
