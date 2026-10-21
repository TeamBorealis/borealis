import { expect } from 'e2e';
import { realClick } from './real-click.js';
import type { Locator } from 'e2e';
import type { Ctx } from './flows-a.js';

/** The web test fixtures the flows use. */
export type FlowCtx = Ctx;

/** Viewport-level key press (working call: `browser.keyboard.press('Control+s' | 'F8' | 'Escape' | 'Alt+q')`). */
export function pressKey(ctx: FlowCtx, key: string): Promise<void> {
  return ctx.browser.keyboard.press(key);
}

/** Text the status bar is showing (innerText, so CSS uppercase is included). */
export function statusText(ctx: FlowCtx): Promise<string> {
  return ctx.browser.evaluate((_: null) => (document.querySelector('.status-msg') as HTMLElement | null)?.innerText ?? '', null);
}

/** Poll until the status bar text satisfies `pred`; returns the final text. */
export async function waitStatus(ctx: FlowCtx, pred: (s: string) => boolean, timeoutMs = 5000): Promise<string> {
  const end = Date.now() + timeoutMs;
  let s = await statusText(ctx);
  while (!pred(s) && Date.now() < end) {
    await new Promise((r) => setTimeout(r, 100));
    s = await statusText(ctx);
  }
  return s;
}

/** Double-click a character tile on /#/characters (real pointer events via the locator). */
export async function openCharacterTile(ctx: FlowCtx, name: string): Promise<void> {
  const tile = ctx.browser.locator(`.v4-tile[aria-label="${name}"] .v4-tile-open`);
  await tile.waitFor({ state: 'visible' });
  await tile.dblclick();
  await expect(ctx.screen.getByRole('menuitem', 'File')).toBeVisible();
}

/** Current location hash. */
export function hash(ctx: FlowCtx): Promise<string> {
  return ctx.browser.evaluate((_: null) => location.hash, null);
}

/** Computed style value of the first match, or '' when absent. */
export function styleOf(ctx: FlowCtx, css: string, prop: string): Promise<string> {
  return ctx.browser.evaluate(({ css, prop }) => {
    const el = document.querySelector(css);
    return el ? getComputedStyle(el).getPropertyValue(prop) : '';
  }, { css, prop });
}

/** Texts of all elements matching `css` (innerText, trimmed). */
export function textsOf(ctx: FlowCtx, css: string): Promise<string[]> {
  return ctx.browser.evaluate((css: string) => Array.from(document.querySelectorAll(css)).map((e) => (e as HTMLElement).innerText.trim()), css);
}

export { realClick };

/**
 * Real-pointer click on a locator with a text-specific hit test: the topmost element at the centre
 * must be (inside) an element whose trimmed innerText equals `text`, otherwise fails naming the
 * covering element (e.g. a scrim over a menu item). Then clicks that point with the real mouse.
 */
export async function realClickText(ctx: FlowCtx, locator: Locator, css: string, text: string): Promise<void> {
  await locator.waitFor({ state: 'visible' });
  const box = await locator.boundingBox();
  if (!box) throw new Error(`realClickText: no box for ${css}`);
  const point = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  const hit = await ctx.browser.evaluate(({ css, text, x, y }) => {
    const top = document.elementFromPoint(x, y);
    const target = Array.from(document.querySelectorAll(css)).find((e) => (e as HTMLElement).innerText.trim() === text);
    const name = top ? `${top.tagName.toLowerCase()}.${String((top as HTMLElement).className).trim().split(/\s+/).join('.')}` : 'null';
    return { ok: !!top && !!target && (target === top || target.contains(top)), top: name };
  }, { css, text, ...point });
  if (!hit.ok) throw new Error(`realClickText: "${text}" is covered at (${Math.round(point.x)}, ${Math.round(point.y)}) by ${hit.top}`);
  await ctx.screen.tapAt(point);
}
