import { settledBox } from './settle.js';
import type { BrowserLike, LocatorLike } from './types.js';

/** Box edges and sizes are exact at the fixed window size: a 1 px box shift is a finding (e.g. a grid edge at 220 instead of 219). */
export const LAYOUT_TOLERANCE_PX = 0;
/** Text-run positions may drift one px with glyph metrics (the reference noise rule for ink offsets). */
export const TEXT_TOLERANCE_PX = 1;

/**
 * Edges of an element in CSS px at the fixed 1355x847 window. `right` and `bottom` are the LAST painted
 * pixel (x + width - 1), the convention the reference measurements use ("right edge 1353").
 */
export interface Edges { left: number; top: number; right: number; bottom: number; width: number; height: number }

interface LayoutCtx { browser: BrowserLike & { locator(css: string): unknown } }

function toEdges(b: { x: number; y: number; width: number; height: number }): Edges {
  const round = Math.round;
  return {
    left: round(b.x), top: round(b.y), right: round(b.x + b.width) - 1, bottom: round(b.y + b.height) - 1,
    width: round(b.width), height: round(b.height),
  };
}

/** Edges of the n-th match, read once (for static pages). Throws when nothing matches. */
export async function edgesOf(ctx: LayoutCtx, css: string, n = 0): Promise<Edges> {
  const b = await ctx.browser.evaluate(({ css, n }) => {
    const el = document.querySelectorAll(css)[n];
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, width: r.width, height: r.height };
  }, { css, n });
  if (!b) throw new Error(`edgesOf: no element for ${css}[${n}]`);
  return toEdges(b);
}

/** Edges of the first match once it has stopped moving (flyouts slide in). */
export async function settledEdgesOf(ctx: LayoutCtx, css: string): Promise<Edges> {
  const loc = ctx.browser.locator(css) as { first(): LocatorLike };
  return toEdges(await settledBox(loc.first()));
}

/** Computed values of `props` on the first match of `css`. Throws when nothing matches. */
export async function cssOf(ctx: LayoutCtx, css: string, props: readonly string[]): Promise<Record<string, string>> {
  const got = await ctx.browser.evaluate(({ css, props }) => {
    const el = document.querySelector(css);
    if (!el) return null;
    const cs = getComputedStyle(el);
    return Object.fromEntries(props.map((p) => [p, cs.getPropertyValue(p).trim()]));
  }, { css, props: [...props] });
  if (!got) throw new Error(`cssOf: no element for ${css}`);
  return got;
}

/** Left edge (CSS px, rounded) of the first text run inside the first match: where the label's glyph box starts. */
export async function textLeftOf(ctx: LayoutCtx, css: string): Promise<number> {
  const x = await ctx.browser.evaluate((css: string) => {
    const el = document.querySelector(css);
    if (!el) return null;
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, {
      acceptNode: (n) => ((n.textContent ?? '').trim() ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP),
    });
    const node = walker.nextNode();
    if (!node) return null;
    const range = document.createRange();
    range.selectNodeContents(node);
    return range.getBoundingClientRect().left;
  }, css);
  if (x === null) throw new Error(`textLeftOf: no text in ${css}`);
  return Math.round(x);
}

/** Value of a custom property declared on :root (the design tokens). */
export function rootToken(ctx: LayoutCtx, name: string): Promise<string> {
  return ctx.browser.evaluate((name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim(), name);
}

/**
 * Reference check, reference value first. Numbers compare within `tolerance` px; strings (colours, font sizes,
 * labels) compare exactly after removing whitespace and case. A mismatch is a U7 regression: never widen a
 * tolerance to make it pass.
 */
export function expectLayout(name: string, reference: number | string, measured: number | string, tolerance = LAYOUT_TOLERANCE_PX): void {
  const label = `U7 ${name}: reference ${reference}, measured ${measured}`;
  if (typeof reference === 'number' && typeof measured === 'number') {
    if (Math.abs(reference - measured) > tolerance) throw new Error(`${label} (allowed +-${tolerance}px)`);
    return;
  }
  const norm = (s: number | string) => String(s).replace(/\s+/g, '').toLowerCase();
  if (norm(reference) !== norm(measured)) throw new Error(label);
}

/** `#rrggbb` (or `#rrggbbaa`) as the `rgb()`/`rgba()` string getComputedStyle returns. */
export function rgb(hex: string): string {
  const h = hex.replace('#', '');
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
  if (h.length === 8) {
    const a = Math.round((parseInt(h.slice(6, 8), 16) / 255) * 100) / 100;
    return `rgba(${r}, ${g}, ${b}, ${a})`;
  }
  return `rgb(${r}, ${g}, ${b})`;
}
