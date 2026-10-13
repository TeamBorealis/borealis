import { expect } from 'e2e';
import { realClick } from './real-click.js';

/** Structural slice of the fixtures (kept loose so helpers do not depend on unexported types). */
export interface Ctx {
  app: { open(path?: string): Promise<void> };
  screen: { tapAt(p: { x: number; y: number }): Promise<void>; getByRole(role: string, name?: string, opts?: { exact?: boolean }): any };
  browser: {
    evaluate<R, A>(fn: (arg: A) => R, arg: A): Promise<R>;
    locator(css: string): any;
    keyboard: { press(key: string): Promise<void> };
  };
}

const POLL_MS = 100;
const WAIT_MS = 5000;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Status-bar message in its authored case (CSS uppercases it for display; textContent is the source string). */
export function statusMsg(ctx: Ctx): Promise<string> {
  return ctx.browser.evaluate((_: null) => (document.querySelector('footer.aur-status .status-msg')?.textContent ?? '').trim(), null);
}

/** Poll until the status message satisfies `pred`; returns the last text seen. */
export async function waitStatus(ctx: Ctx, pred: (s: string) => boolean, timeoutMs = WAIT_MS): Promise<string> {
  const end = Date.now() + timeoutMs;
  let s = await statusMsg(ctx);
  while (!pred(s) && Date.now() < end) { await sleep(POLL_MS); s = await statusMsg(ctx); }
  return s;
}

/** Fail with the last status text when it never becomes `want` (exact, authored case). */
export async function expectStatus(ctx: Ctx, want: string): Promise<void> {
  const got = await waitStatus(ctx, (s) => s === want);
  if (got !== want) throw new Error(`status bar: wanted "${want}" but saw "${got}"`);
}

/** Generic poll for a page-side predicate. */
export async function waitFor<A>(ctx: Ctx, fn: (arg: A) => boolean, arg: A, what: string, timeoutMs = WAIT_MS): Promise<void> {
  const end = Date.now() + timeoutMs;
  for (;;) {
    if (await ctx.browser.evaluate(fn, arg)) return;
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
    await sleep(POLL_MS);
  }
}

/** Real-pointer click on the first element matching `css` (hit-tested; a covering element fails the step). */
export function clickCss(ctx: Ctx, css: string): Promise<void> {
  return realClick(ctx as never, ctx.browser.locator(css).first(), css);
}

/** Hit-tested double-click (a real double-click needs the browser's click-count, so it goes through the locator). */
export async function dblClickCss(ctx: Ctx, css: string): Promise<void> {
  const loc = ctx.browser.locator(css).first();
  await loc.waitFor({ state: 'visible' });
  const box = await loc.boundingBox();
  const hit = await ctx.browser.evaluate(({ css, x, y }) => {
    const top = document.elementFromPoint(x, y);
    return !!top && Array.from(document.querySelectorAll(css)).some((t) => t === top || t.contains(top));
  }, { css, x: box.x + box.width / 2, y: box.y + box.height / 2 });
  if (!hit) throw new Error(`dblClickCss: ${css} is covered at its centre`);
  await loc.dblclick();
}

/** Real-pointer click on the centre of the n-th (0-based) element matching `css`, e.g. one cell of a row. */
export async function clickNth(ctx: Ctx, css: string, n: number): Promise<void> {
  const box = await ctx.browser.locator(css).nth(n).boundingBox();
  if (!box) throw new Error(`clickNth: no box for ${css}[${n}]`);
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  const hit = await ctx.browser.evaluate(({ css, n, x, y }) => {
    const top = document.elementFromPoint(x, y);
    const el = document.querySelectorAll(css)[n];
    return !!top && !!el && (el === top || el.contains(top));
  }, { css, n, x, y });
  if (!hit) throw new Error(`clickNth: ${css}[${n}] is covered at (${Math.round(x)}, ${Math.round(y)})`);
  await ctx.screen.tapAt({ x, y });
}

/** Number of elements matching `css`. */
export function count(ctx: Ctx, css: string): Promise<number> {
  return ctx.browser.evaluate((css: string) => document.querySelectorAll(css).length, css);
}

/** Visible text of the first match (innerText), '' when absent. */
export function textOf(ctx: Ctx, css: string): Promise<string> {
  return ctx.browser.evaluate((css: string) => (document.querySelector(css) as HTMLElement | null)?.innerText ?? '', css);
}

/** Authored text of the first match (textContent: not affected by CSS text-transform). */
export function rawTextOf(ctx: Ctx, css: string): Promise<string> {
  return ctx.browser.evaluate((css: string) => (document.querySelector(css)?.textContent ?? '').trim(), css);
}

/** Whether the first match is disabled (null when absent). */
export function isDisabled(ctx: Ctx, css: string): Promise<boolean | null> {
  return ctx.browser.evaluate((css: string) => {
    const el = document.querySelector(css) as HTMLButtonElement | null;
    return el ? el.disabled : null;
  }, css);
}

/** Current location hash without the leading `#`. */
export function route(ctx: Ctx): Promise<string> {
  return ctx.browser.evaluate((_: null) => location.hash.replace(/^#/, ''), null);
}

/** Computed style value of the first match. */
export function style(ctx: Ctx, css: string, prop: string): Promise<string> {
  return ctx.browser.evaluate(({ css, prop }) => {
    const el = document.querySelector(css);
    if (!el) throw new Error(`style: no element for ${css}`);
    return getComputedStyle(el).getPropertyValue(prop);
  }, { css, prop });
}

/** Box of the n-th match as plain numbers. */
export function boxOf(ctx: Ctx, css: string, n = 0): Promise<{ x: number; y: number; width: number; height: number }> {
  return ctx.browser.evaluate(({ css, n }) => {
    const el = document.querySelectorAll(css)[n];
    if (!el) throw new Error(`boxOf: no element for ${css}[${n}]`);
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, width: r.width, height: r.height };
  }, { css, n });
}

/** Click the sub-tab whose text is `name` (case-insensitive exact), with a real pointer. */
export async function clickSubTab(ctx: Ctx, name: string): Promise<void> {
  const idx = await ctx.browser.evaluate((name: string) => {
    const tabs = Array.from(document.querySelectorAll('[role=tab]'));
    return tabs.findIndex((t) => (t.textContent ?? '').trim().toLowerCase() === name.toLowerCase());
  }, name);
  if (idx < 0) throw new Error(`clickSubTab: no tab "${name}"`);
  await clickNth(ctx, '[role=tab]', idx);
}

/** Click a main-nav tab (Start, Build, Magic, Equipment, ...) by name. */
export async function clickMainTab(ctx: Ctx, name: string): Promise<void> {
  const idx = await ctx.browser.evaluate((name: string) => Array.from(document.querySelectorAll('a.tab')).findIndex((t) => (t.textContent ?? '').trim() === name), name);
  if (idx < 0) throw new Error(`clickMainTab: no tab "${name}"`);
  await clickNth(ctx, 'a.tab', idx);
}

/** Index of the first element matching `css` whose text contains `text`; -1 when none. */
export function indexByText(ctx: Ctx, css: string, text: string): Promise<number> {
  return ctx.browser.evaluate(({ css, text }) => Array.from(document.querySelectorAll(css)).findIndex((e) => (e.textContent ?? '').includes(text)), { css, text });
}

/** Click (real pointer) the first `css` element whose text contains `text`. */
export async function clickByText(ctx: Ctx, css: string, text: string): Promise<void> {
  const idx = await indexByText(ctx, css, text);
  if (idx < 0) throw new Error(`clickByText: no ${css} containing "${text}"`);
  await clickNth(ctx, css, idx);
}

/** Boot to the start screen and open the seeded Seraphine Vale (level 2 Elf Wizard) with a double-click. */
export async function loadSeraphine(ctx: Ctx): Promise<void> {
  await ctx.app.open('/#/characters');
  await expect(ctx.screen.getByRole('menuitem', 'File')).toBeVisible();
  await dblClickCss(ctx, '.v4-tile-open[aria-label="Seraphine Vale, Level 2 Elf Wizard"]');
  await waitStatus(ctx, (s) => s.startsWith("Loaded"));
}

/** Open the Level up flyout from the quick bar. */
export async function openLevelUp(ctx: Ctx): Promise<void> {
  const idx = await indexByText(ctx, 'button', 'Level Up');
  if (idx < 0) throw new Error('openLevelUp: no Level Up button');
  await clickNth(ctx, 'button', idx);
  await expect(ctx.screen.getByRole('dialog', 'Level up & multiclassing')).toBeVisible();
}

/** Hit-tested double-click on the first `css` element whose text contains `text` (e.g. a picker row). */
export async function dblClickByText(ctx: Ctx, css: string, text: string): Promise<void> {
  const idx = await indexByText(ctx, css, text);
  if (idx < 0) throw new Error(`dblClickByText: no ${css} containing "${text}"`);
  const loc = ctx.browser.locator(css).nth(idx);
  await loc.waitFor({ state: 'visible' });
  const box = await loc.boundingBox();
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  const hit = await ctx.browser.evaluate(({ css, idx, x, y }) => {
    const top = document.elementFromPoint(x, y);
    const el = document.querySelectorAll(css)[idx];
    return !!top && !!el && (el === top || el.contains(top));
  }, { css, idx, x, y });
  if (!hit) throw new Error(`dblClickByText: ${css} "${text}" is covered at its centre`);
  await loc.dblclick();
}

/**
 * Real-pointer click on a non-name cell: finds the first `rowCss` row containing `rowText`, takes its `cellCss` cell,
 * hit-tests the centre and taps it with the real mouse. A single press (mouse-down + up) is what full-row selection must accept.
 */
export async function clickCellInRow(ctx: Ctx, rowCss: string, rowText: string, cellCss: string): Promise<void> {
  const pt = await ctx.browser.evaluate(({ rowCss, rowText, cellCss }) => {
    const row = Array.from(document.querySelectorAll(rowCss)).find((r) => (r.textContent ?? '').includes(rowText));
    if (!row) return { err: `no ${rowCss} containing "${rowText}"` };
    const cell = row.querySelector(cellCss);
    if (!cell) return { err: `no ${cellCss} in that row` };
    const r = cell.getBoundingClientRect();
    const x = r.x + r.width / 2;
    const y = r.y + r.height / 2;
    const top = document.elementFromPoint(x, y);
    if (!top || !(cell === top || cell.contains(top))) return { err: `${cellCss} is covered at (${Math.round(x)}, ${Math.round(y)})` };
    return { x, y };
  }, { rowCss, rowText, cellCss });
  if ('err' in pt) throw new Error(`clickCellInRow: ${pt.err}`);
  await ctx.screen.tapAt({ x: pt.x as number, y: pt.y as number });
}

/** Text of the rows matching `css`, whitespace collapsed. */
export function rowsText(ctx: Ctx, css: string): Promise<string[]> {
  return ctx.browser.evaluate((css: string) => Array.from(document.querySelectorAll(css)).map((r) => ((r as HTMLElement).innerText ?? '').replace(/\s+/g, ' ').trim()), css);
}
