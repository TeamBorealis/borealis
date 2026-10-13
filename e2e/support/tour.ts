import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

/**
 * Helpers for the human-paced "tour" recordings (tests/tour). Demos, not tests: a caption banner,
 * a visible cursor that glides to each click target, pauses after visible changes. Everything is
 * injected into the page at run time, so nothing exists outside tour runs.
 */
interface TourBase {
  app: { open(path?: string): Promise<void> };
  screen: { tapAt(p: { x: number; y: number }): Promise<void> };
  browser: { evaluate<R, A>(fn: (arg: A) => R, arg: A): Promise<R>; locator(css: string): any };
}

export interface CaptionEntry { text: string; t: number }
export interface TourLog { name: string; title: string; startedAt: number; captions: CaptionEntry[]; problems: string[] }

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const GLIDE_MS = 650;
const BEFORE_CLICK_MS = 180;
const TYPE_DELAY_MS = 90;
const OUT_DIR = join(process.cwd(), '.e2e', 'tour');
const logs = new WeakMap<object, TourLog>();

/** Pause so a viewer can see what just changed. */
export function pace(_ctx: unknown, ms = 900): Promise<void> {
  return sleep(ms);
}

/** Inject (idempotently) the caption banner and the fake cursor. */
async function ensureOverlay(ctx: TourBase): Promise<void> {
  await ctx.browser.evaluate((_: null) => {
    if (document.getElementById('tour-banner')) return 1;
    const banner = document.createElement('div');
    banner.id = 'tour-banner';
    banner.style.cssText = 'position:fixed;left:50%;bottom:34px;transform:translateX(-50%);z-index:2147483647;pointer-events:none;'
      + 'background:rgba(0,0,0,0.78);color:#fff;font:15px/1.35 -apple-system,Helvetica,Arial,sans-serif;padding:8px 18px;border-radius:6px;'
      + 'max-width:80%;text-align:center;opacity:0;transition:opacity 150ms';
    const cursor = document.createElement('div');
    cursor.id = 'tour-cursor';
    cursor.style.cssText = 'position:fixed;left:40px;top:40px;width:18px;height:18px;margin:-9px 0 0 -9px;z-index:2147483646;pointer-events:none;'
      + 'border-radius:50%;background:rgba(255,196,0,0.55);border:2px solid #fff;box-shadow:0 0 0 1px #000,0 0 8px rgba(0,0,0,0.6);'
      + 'transition:left 650ms cubic-bezier(.4,0,.2,1),top 650ms cubic-bezier(.4,0,.2,1),transform 120ms';
    document.body.append(banner, cursor);
    return 1;
  }, null);
}

/** Show a caption in the banner and record it with its offset from the tour start. */
export async function caption(ctx: TourBase, text: string): Promise<void> {
  await ensureOverlay(ctx);
  const log = logs.get(ctx.app);
  if (log) log.captions.push({ text, t: Date.now() - log.startedAt });
  await ctx.browser.evaluate((t: string) => {
    const b = document.getElementById('tour-banner');
    if (b) { b.textContent = t; b.style.opacity = '1'; }
    return 1;
  }, text);
  await sleep(1100);
}

async function glideTo(ctx: TourBase, x: number, y: number): Promise<void> {
  await ensureOverlay(ctx);
  await ctx.browser.evaluate(({ x, y }) => {
    const c = document.getElementById('tour-cursor');
    if (c) { c.style.left = `${x}px`; c.style.top = `${y}px`; }
    return 1;
  }, { x, y });
  await sleep(GLIDE_MS + BEFORE_CLICK_MS);
}

async function pulse(ctx: TourBase): Promise<void> {
  await ctx.browser.evaluate((_: null) => {
    const c = document.getElementById('tour-cursor');
    if (c) { c.style.transform = 'scale(1.8)'; setTimeout(() => { c.style.transform = ''; }, 160); }
    return 1;
  }, null);
}

/**
 * Wrap a test context so every real-pointer click (`screen.tapAt`, used by realClick and all the
 * flows helpers) first glides the visible cursor to the target. Starts the tour log.
 */
export function startTour<C extends TourBase>(ctx: C, name: string, title: string): C {
  logs.set(ctx.app, { name, title, startedAt: Date.now(), captions: [], problems: [] });
  const screen = new Proxy(ctx.screen as object, {
    get(target, prop, recv) {
      if (prop === 'tapAt') {
        return async (p: { x: number; y: number }) => {
          await glideTo(ctx, p.x, p.y);
          await pulse(ctx);
          await (ctx.screen as TourBase['screen']).tapAt(p);
        };
      }
      const v = Reflect.get(target, prop, recv);
      return typeof v === 'function' ? v.bind(target) : v;
    },
  });
  return new Proxy(ctx as object, { get: (t, p, r) => (p === 'screen' ? screen : Reflect.get(t, p, r)) }) as C;
}

/** Glide the cursor onto the centre of `css`, then double-click it with the locator. */
export async function tourDblClick(ctx: TourBase, css: string, nth = 0): Promise<void> {
  const loc = ctx.browser.locator(css).nth(nth);
  await loc.waitFor({ state: 'visible' });
  const box = await loc.boundingBox();
  await glideTo(ctx, box.x + box.width / 2, box.y + box.height / 2);
  await pulse(ctx);
  await loc.dblclick();
}

/** Glide to the centre of `css` and stay there (hover feel) without clicking. */
export async function tourPoint(ctx: TourBase, css: string, nth = 0): Promise<void> {
  const box = await ctx.browser.locator(css).nth(nth).boundingBox();
  if (box) await glideTo(ctx, box.x + box.width / 2, box.y + box.height / 2);
}

/** Type text one character at a time into the (already focused) field. */
export async function tourType(ctx: TourBase & { browser: { keyboard?: { type(t: string): Promise<void> } } }, css: string, text: string): Promise<void> {
  const loc = ctx.browser.locator(css).first();
  await loc.waitFor({ state: 'visible' });
  const box = await loc.boundingBox();
  await glideTo(ctx, box.x + box.width / 2, box.y + box.height / 2);
  await (ctx.screen as TourBase['screen']).tapAt({ x: box.x + box.width / 2, y: box.y + box.height / 2 });
  await loc.fill('');
  for (let i = 1; i <= text.length; i++) {
    await loc.fill(text.slice(0, i));
    await sleep(TYPE_DELAY_MS);
  }
}

/** Run an optional demo step; a failure is logged and the tour carries on. */
export async function soft(ctx: TourBase, label: string, fn: () => Promise<void>): Promise<void> {
  try {
    await fn();
  } catch (e) {
    const msg = `${label}: ${(e as Error).message.split('\n')[0]}`;
    logs.get(ctx.app)?.problems.push(msg);
    console.warn(`[tour] ${msg}`);
  }
}

/** Write the caption log next to the run output; the collect script reads it. */
export function finishTour(ctx: TourBase): void {
  const log = logs.get(ctx.app);
  if (!log) return;
  mkdirSync(OUT_DIR, { recursive: true });
  writeFileSync(join(OUT_DIR, `${log.name}.json`), JSON.stringify({ ...log, durationMs: Date.now() - log.startedAt }, null, 2));
}

/** Glide to a locator (e.g. `screen.getByRole(...)`) and click it with the real mouse; `secondary` right-clicks it. */
export async function tourClickLoc(ctx: TourBase, loc: any, opts: { secondary?: boolean; modifiers?: string[] } = {}): Promise<void> {
  await loc.waitFor({ state: 'visible' });
  const box = await loc.boundingBox();
  await glideTo(ctx, box.x + box.width / 2, box.y + box.height / 2);
  await pulse(ctx);
  if (opts.secondary) await loc.secondaryTap();
  else if (opts.modifiers) await loc.tap({ modifiers: opts.modifiers });
  else await (ctx.screen as TourBase['screen']).tapAt({ x: box.x + box.width / 2, y: box.y + box.height / 2 });
}
