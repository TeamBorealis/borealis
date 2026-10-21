import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, cssOf, edgesOf, expectLayout, realClick, rgb, rootToken, settledEdgesOf, TEXT_TOLERANCE_PX, textLeftOf } from '../support/index.js';
import { clickMainTab, loadSeraphine, openLevelUp } from '../support/flows-a.js';
import { pressKey } from '../support/flows-b.js';

// U7 (UI matches the classic builder "down to a dotted i"): shell chrome, design tokens and the shared flyout frame.
// Every number is a reference measurement at the fixed 1355x847 window (1 px window border), reference value first.
// Deterministic: MockApi, no model.

const WINDOW = { width: 1355, height: 847 } as const;
const ACCENT = '#293338';
const CHROME = '#21292d';
const FONT = 'Selawik';

const TOKENS: [string, string][] = [
  ['--accent-base', ACCENT], ['--accent-80', '#293338cc'], ['--accent-60', '#29333899'], ['--accent-40', '#29333866'], ['--accent-20', '#29333833'],
  ['--gold', '#b38000'], ['--bg', '#161616'], ['--window-bg', '#101010'], ['--flyout-bg', '#151516'], ['--description-bg', '#101010'],
  ['--subnav-bg', '#111111'], ['--subnav-border', '#242424'], ['--chrome', CHROME], ['--menu-bg', '#1d1d20'], ['--input-border', '#a59f93'],
  ['--aur-success', '#37753d'], ['--aur-warning', '#cf6e39'], ['--aur-danger', '#943131'], ['--gray-5', '#b9b9b9'], ['--gray-6', '#cccccc'],
  ['--title-h', '30px'], ['--maintab-h', '36px'], ['--subtab-h', '30px'], ['--status-h', '26px'], ['--scrollbar', '17px'], ['--radius', '2px'],
  ['--fs-tab-main', '18px'], ['--fs-tab-sub', '14px'],
];

type Ctx = Parameters<typeof bootToHome>[0] & Parameters<typeof edgesOf>[0];

/**
 * The shared right-flyout frame: `width` px flush with the window's inner right edge, header box y20-61.
 * `titlePx` is 18 except where the title is tuned to the reference ink width (Level up: 17.5).
 */
async function expectRightFlyout(ctx: Ctx, what: string, width: number, titlePx = '18px'): Promise<void> {
  const panel = await settledEdgesOf(ctx, '.flyout');
  expectLayout(`${what} flyout width`, width, panel.width);
  expectLayout(`${what} flyout right edge`, WINDOW.width - 2, panel.right);
  expectLayout(`${what} flyout top`, 1, panel.top);
  const head = await edgesOf(ctx, '.flyout-head');
  expectLayout(`${what} header top rule`, 20, head.top);
  expectLayout(`${what} header bottom rule`, 61, head.bottom);
  expectLayout(`${what} header left inset`, panel.left + 21, head.left);
  const headCss = await cssOf(ctx, '.flyout-head', ['background-color', 'border-top-color']);
  expectLayout(`${what} header fill`, rgb('#29333833'), headCss['background-color']!);
  expectLayout(`${what} header border`, rgb('#29333866'), headCss['border-top-color']!);
  const title = await cssOf(ctx, '.flyout-head h2', ['font-size', 'font-weight', 'color', 'text-transform']);
  expectLayout(`${what} title size`, titlePx, title['font-size']!);
  expectLayout(`${what} title weight`, '600', title['font-weight']!);
  expectLayout(`${what} title colour`, rgb('#b9b9b9'), title.color!);
  expectLayout(`${what} title case`, 'uppercase', title['text-transform']!);
  const frame = await cssOf(ctx, '.flyout', ['background-color', 'border-left-color']);
  expectLayout(`${what} flyout fill`, rgb('#151516'), frame['background-color']!);
  expectLayout(`${what} flyout border`, rgb(ACCENT), frame['border-left-color']!);
  const scrim = await edgesOf(ctx, '.flyout-overlay');
  expectLayout(`${what} scrim inset`, 1, scrim.left);
  expectLayout(`${what} scrim colour`, 'rgba(245, 245, 245, 0.5)', (await cssOf(ctx, '.flyout-overlay', ['background-color']))['background-color']!);
}

async function closeFlyout(ctx: Ctx): Promise<void> {
  await pressKey(ctx as never, 'Escape');
  await ctx.browser.evaluate((_: null) => new Promise((r) => setTimeout(r, 300)), null);
}

describe('u7 shell', { tags: ['u7'] }, () => {
  test('design tokens keep the reference palette, metrics and font', async (ctx) => {
    await bootToHome(ctx);
    for (const [name, value] of TOKENS) expectLayout(`token ${name}`, value, await rootToken(ctx, name));
    const family = (await cssOf(ctx, 'body', ['font-family']))['font-family']!;
    expectLayout('body font family', FONT, family.split(',')[0]!.replace(/"/g, ''));
  });

  test('window rows: title 30, profile + main tabs 61, sub-tab band 30, content, status 26', async (ctx) => {
    await bootToHome(ctx);
    const win = await edgesOf(ctx, '.aur-window');
    expectLayout('window width', WINDOW.width, win.width);
    expectLayout('window height', WINDOW.height, win.height);
    expectLayout('window border', rgb(ACCENT), (await cssOf(ctx, '.aur-window', ['border-top-color']))['border-top-color']!);
    const title = await edgesOf(ctx, '.aur-title');
    expectLayout('title bar top', 1, title.top);
    expectLayout('title bar bottom', 29, title.bottom);
    const chrome = await edgesOf(ctx, '.aur-chrome');
    expectLayout('main tab band top', 30, chrome.top);
    expectLayout('main tab band bottom (2 px accent rule)', 90, chrome.bottom);
    const band = await edgesOf(ctx, '.aur-subband');
    expectLayout('sub-tab band top', 91, band.top);
    expectLayout('sub-tab band height', 30, band.height);
    const content = await edgesOf(ctx, '.aur-content');
    expectLayout('content top', 121, content.top);
    const status = await edgesOf(ctx, '.aur-status');
    expectLayout('status bar height', 26, status.height);
    expectLayout('status bar bottom', WINDOW.height - 2, status.bottom);
    expectLayout('content bottom meets the status bar', status.top - 1, content.bottom);
    for (const css of ['.aur-title', '.aur-chrome', '.aur-status']) {
      expectLayout(`${css} fill`, rgb(CHROME), (await cssOf(ctx, css, ['background-color']))['background-color']!);
    }
    const bandCss = await cssOf(ctx, '.aur-subband', ['background-color', 'border-bottom-color']);
    expectLayout('sub-tab band fill', rgb('#111111'), bandCss['background-color']!);
    expectLayout('sub-tab band rule', rgb('#242424'), bandCss['border-bottom-color']!);
    expectLayout('main tab rule', rgb(ACCENT), (await cssOf(ctx, '.aur-chrome', ['border-bottom-color']))['border-bottom-color']!);
  });

  test('title bar, menus, tab strips and status text use the reference type', async (ctx) => {
    await bootToHome(ctx);
    const menu = await cssOf(ctx, '.aur-menu-top', ['font-size', 'font-weight', 'color', 'text-transform']);
    expectLayout('menu size', '12px', menu['font-size']!);
    expectLayout('menu weight', '600', menu['font-weight']!);
    expectLayout('menu colour', rgb('#b9b9b9'), menu.color!);
    expectLayout('menu case', 'uppercase', menu['text-transform']!);
    const titleText = await cssOf(ctx, '.aur-title-text', ['font-size', 'font-weight', 'color']);
    expectLayout('window title size', '12px', titleText['font-size']!);
    expectLayout('window title colour', rgb('#b9b9b9'), titleText.color!);
    const main = await cssOf(ctx, '.tabs.main .tab', ['font-size', 'font-weight', 'text-transform']);
    expectLayout('main tab size', '18px', main['font-size']!);
    expectLayout('main tab weight', '600', main['font-weight']!);
    expectLayout('main tab case', 'uppercase', main['text-transform']!);
    const mainTab = await edgesOf(ctx, '.tabs.main .tab');
    expectLayout('main tab height', 36, mainTab.height);
    expectLayout('first main tab label x', 51, await textLeftOf(ctx, '.tabs.main .tab'), TEXT_TOLERANCE_PX);
    const sub = await cssOf(ctx, '.aur-subband .tabs.sub .tab[aria-current="page"], .aur-subband .tabs.sub .tab[aria-selected="true"]', ['font-size', 'font-weight', 'color']);
    expectLayout('sub-tab size', '14px', sub['font-size']!);
    expectLayout('sub-tab weight', '600', sub['font-weight']!);
    expectLayout('selected sub-tab colour', rgb('#e6e6e6'), sub.color!);
    expectLayout('first sub-tab label x', 51, await textLeftOf(ctx, '.aur-subband .tabs.sub .tab'), TEXT_TOLERANCE_PX);
    const status = await cssOf(ctx, '.aur-status', ['font-size', 'color', 'text-transform']);
    expectLayout('status size', '11px', status['font-size']!);
    expectLayout('status colour', rgb('#cccccc'), status.color!);
    expectLayout('status case', 'uppercase', status['text-transform']!);
    const search = await edgesOf(ctx, '.aur-search');
    expectLayout('quick search width', 225, search.width);
    const newChar = await edgesOf(ctx, '.aur-newchar');
    expectLayout('new character button size', 44, newChar.width);
    expectLayout('new character button height', 44, newChar.height);
  });

  test('loaded character: gold name menu, 44 px portrait, two-pane gutter, nothing between band and content', async (ctx) => {
    await loadSeraphine(ctx);
    expectLayout('character name menu colour', rgb('#b38000'), (await cssOf(ctx, '.aur-menu-top.gold', ['color'])).color!);
    const portrait = await edgesOf(ctx, '.aur-portrait-btn');
    expectLayout('portrait button width', 44, portrait.width);
    expectLayout('portrait button height', 44, portrait.height);
    const name = await cssOf(ctx, '.aur-profile .name', ['font-weight', 'color', 'text-transform']);
    expectLayout('profile name weight', '600', name['font-weight']!);
    expectLayout('profile name colour', rgb('#cccccc'), name.color!);
    expectLayout('profile name case', 'uppercase', name['text-transform']!);
    // Two-pane pages: a 5 px #131313 strip at x1349-1353 right of the description pane.
    const content = await cssOf(ctx, '.aur-content', ['border-right-width', 'border-right-color']);
    expectLayout('two-pane strip width', '5px', content['border-right-width']!);
    expectLayout('two-pane strip colour', rgb('#131313'), content['border-right-color']!);
    expectLayout('description pane right edge', WINDOW.width - 2, (await edgesOf(ctx, '.bp-right')).right);
    // No banner or bar is inserted under the sub-tab band (the update notice is a title-bar command).
    expectLayout('build content top', 121, (await edgesOf(ctx, '.aur-content')).top);
    expectLayout('no update bar', 0, await ctx.browser.evaluate((_: null) => document.querySelectorAll('.aur-update').length, null));
    await clickMainTab(ctx, 'Equipment');
    expectLayout('equipment content top', 121, (await edgesOf(ctx, '.aur-content')).top);
  });

  test('flyout frame: New character, Level up, quick view, portraits, options, save', async (ctx) => {
    const { browser, screen } = ctx;
    await bootToHome(ctx);
    await realClick(ctx, browser.locator('button.aur-newchar'), 'button.aur-newchar');
    await expect(screen.getByRole('dialog', 'New character')).toBeVisible();
    await expectRightFlyout(ctx, 'New character', 400);
    await closeFlyout(ctx);

    await loadSeraphine(ctx);
    await openLevelUp(ctx);
    await expectRightFlyout(ctx, 'Level up', 400, '17.5px');
    await closeFlyout(ctx);
    await realClick(ctx, browser.locator('.aur-profile .name'), '.aur-profile .name');
    await expectRightFlyout(ctx, 'Quick view', 400);
    await closeFlyout(ctx);
    await pressKey(ctx, 'Control+p');
    await expect(screen.getByRole('dialog', 'Portraits gallery')).toBeVisible();
    const gallery = await settledEdgesOf(ctx, '.flyout');
    if (gallery.width < 430 || gallery.width > 460) throw new Error(`U7 portraits flyout width: reference 430-460, measured ${gallery.width}`);
    await expectRightFlyout(ctx, 'Portraits', gallery.width);
    await closeFlyout(ctx);
    await pressKey(ctx, 'F8');
    await expect(screen.getByRole('dialog', 'Character options')).toBeVisible();
    await expectRightFlyout(ctx, 'Character options', 400);
    await closeFlyout(ctx);
    await pressKey(ctx, 'Control+s');
    await expect(screen.getByRole('dialog', 'Save character')).toBeVisible();
    await expectRightFlyout(ctx, 'Save', 400);
    const foot = await edgesOf(ctx, '.flyout .v4-foot button');
    expectLayout('flyout footer button height', 26, foot.height);
    expectLayout('flyout footer button fill', rgb('#242425'), (await cssOf(ctx, '.flyout .v4-foot button', ['background-color']))['background-color']!);
  });
});
