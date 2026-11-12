import { expect } from 'e2e';
import { pressKey } from './flows-b.js';
import { dblClickCss, statusMsg, waitFor, type Ctx } from './flows-a.js';
import { settledBox } from './settle.js';
import { expectEqual } from './check.js';

// Shared steps of the homebrew creator suites on the web mock (`?creator=1`, desktop 1355x847).

export const NAME = '#hb-f-name-input';

/**
 * Budget for a step that waits on the first render of a lazily loaded creator screen. Vite compiles a route's modules on
 * its first request, which takes seconds while cargo or a device build loads the machine (hc-wb09 under load, board item
 * HvB30); the waits poll the real condition, so a quiet run returns at once.
 */
export const LOAD_MS = 30_000;

export async function waitValue(ctx: Ctx, css: string, want: string, timeoutMs?: number): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`, timeoutMs);
}

export const inputValue = (ctx: Ctx, css: string): Promise<string> =>
  ctx.browser.evaluate((sel: string) => (document.querySelector(sel) as HTMLInputElement | null)?.value ?? '', css);

export const isChecked = (ctx: Ctx, css: string): Promise<boolean> =>
  ctx.browser.evaluate((sel: string) => (document.querySelector(sel) as HTMLInputElement | null)?.checked === true, css);

/** The autosave line says the draft is kept (the draft went to the backend). */
export const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

/** CATEGORIES (the element kinds list) shows `text`, e.g. /Spells \(1\)/. */
export const kindCount = (ctx: Ctx, text: RegExp): Promise<void> =>
  waitFor(ctx, (re: string) => new RegExp(re).test(document.querySelector('[aria-label="Element kinds"]')?.textContent ?? ''), text.source, `kinds ${text.source}`);

export const sectionTitles = (ctx: Ctx): Promise<string[]> =>
  ctx.browser.evaluate((_: null) => [...document.querySelectorAll('.hb-editor .hb-section-title')].map((h) => h.textContent ?? ''), null);

/** The pack's workspace is on screen: its heading and CATEGORIES are rendered (the route's chunk is loaded). */
export async function waitWorkspace(ctx: Ctx, pack: string): Promise<void> {
  await waitFor(ctx, (want: string) =>
    [...document.querySelectorAll('h1, h2, h3')].some((h) => h.textContent === want) && document.querySelector('[aria-label="Element kinds"]') !== null,
  `Homebrew: ${pack}`, `the workspace of ${pack}`, LOAD_MS);
}

/** START > HOMEBREW is on screen with its pack grid (the route's chunk is loaded). */
export async function waitPackList(ctx: Ctx): Promise<void> {
  await waitFor(ctx, (_: null) => document.querySelector('[aria-label="Homebrew packs"]') !== null, null, 'the pack list', LOAD_MS);
}

/** NEW PACK `name` (optionally an edition from the flyout's select) -> its workspace. */
export async function newPack(ctx: Ctx, name: string, edition?: string): Promise<void> {
  await waitPackList(ctx);
  await ctx.screen.getByRole('button', 'New pack').first().tap();
  await expect(ctx.screen.getByRole('dialog', 'New pack')).toBeVisible();
  await ctx.browser.locator('#hb-pack-name').fill(name);
  if (edition !== undefined) await ctx.browser.locator('#hb-pack-edition').selectOption(edition);
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitWorkspace(ctx, name);
}

/**
 * New element of the kind whose card name matches `card` (undefined: the default, a spell), named `name`, from the
 * empty editor's "New element" or the element list's "New".
 */
export async function newElement(ctx: Ctx, name: string, card?: RegExp): Promise<void> {
  await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
  await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
  if (card) await ctx.screen.getByRole('radio', card).tap();
  await ctx.browser.locator('#hb-new-name').fill(name);
  await ctx.screen.getByRole('button', 'Create').tap();
  await waitValue(ctx, NAME, name, LOAD_MS);
}

/** Ctrl+S on the creator route saves the element: its row in the list and the status line. */
export async function save(ctx: Ctx, name: string, pack: string): Promise<void> {
  await autosaved(ctx);
  await pressKey(ctx, 'Control+s');
  await expect(ctx.screen.getByRole('button', name).first()).toBeVisible();
  expectEqual('status after save', `Saved '${name}' to ${pack}.`, await statusMsg(ctx));
}

/** Scrolls an element into the middle of its scroller and waits until it holds still (hit-tested clicks test its centre). */
export async function reveal(ctx: Ctx, css: string): Promise<void> {
  await ctx.browser.evaluate((sel: string) => {
    const el = document.querySelector(sel);
    el?.scrollIntoView({ block: 'center' });
    return el !== null;
  }, css);
  await settledBox(ctx.browser.locator(css).first());
}

/** In the open reference picker: search for `name`, wait until its row is the only one, and double-click it. */
export async function pickRow(ctx: Ctx, name: string): Promise<void> {
  await ctx.browser.locator('.hb-picker .pick-fsearch').fill(name.toLowerCase());
  const row = `.hb-pick-list .pick-row[data-row="${name}"]`;
  await waitFor(ctx, ({ sel }: { sel: string }) => {
    const rows = [...document.querySelectorAll('.hb-pick-list .pick-row')];
    return rows.length === 1 && rows[0] === document.querySelector(sel);
  }, { sel: row }, `only ${name} offered`);
  await settledBox(ctx.browser.locator(`${row} button.pick-main`).first());
  await dblClickCss(ctx, `${row} button.pick-main`);
}

/** RULES > Add rule > `rule` (searched first) -> its block dialog is open. */
export async function addRule(ctx: Ctx, rule: string): Promise<void> {
  await ctx.screen.getByRole('button', 'Add rule').tap();
  await expect(ctx.screen.getByRole('dialog', 'Add a rule')).toBeVisible();
  await ctx.browser.locator('.hb-add-block input[aria-label="Search rules"]').fill(rule.toLowerCase());
  await ctx.screen.getByRole('button', rule).tap();
  await expect(ctx.screen.getByRole('dialog', `New rule: ${rule}`)).toBeVisible();
}

/** Try it is at test level `n` (its status line). */
export const trialLevel = (ctx: Ctx, n: number): Promise<void> =>
  waitFor(ctx, (want: number) => (document.querySelector('.hb-try-status')?.textContent ?? '').includes(`level ${want}`), n, `test character at level ${n}`, LOAD_MS);

/** Done in the child dialog of the form `key` (its body carries `data-child`; not the dialog of a child inside it). */
export async function doneChild(ctx: Ctx, key: string): Promise<void> {
  await ctx.browser.evaluate((k: string) => {
    const own = [...document.querySelectorAll('[role="dialog"]')].find((d) => d.querySelector(`[data-child="${k}"]`)
      && [...d.querySelectorAll('[data-child]')].every((x) => x.getAttribute('data-child') === k));
    const button = [...(own?.querySelectorAll('button') ?? [])].find((b) => b.textContent === 'Done') as HTMLButtonElement | undefined;
    button?.click();
    return button !== undefined;
  }, key);
  await waitFor(ctx, (k: string) => document.querySelector(`[data-child="${k}"]`) === null, key, `the dialog of ${key} closed`);
}

/** Name the child `key` in its dialog. */
export async function nameChild(ctx: Ctx, key: string, name: string): Promise<void> {
  const box = `#hb-f-c-${key}-name-input`;
  await ctx.browser.locator(box).fill(name);
  await waitValue(ctx, box, name);
}

/** The number field `key` (its spin box), set by typing. */
export async function setNumber(ctx: Ctx, key: string, value: number): Promise<void> {
  const box = `[data-field="${key}"] input[role="spinbutton"]`;
  await ctx.browser.locator(box).fill(String(value));
  await waitValue(ctx, box, String(value));
}

/**
 * Chooses `value` in the native select `css` through the page (the input event React listens to). For a select whose
 * locator never reports actionable while the form re-renders around it (the magic item's "Made from" filter).
 */
export async function chooseIn(ctx: Ctx, css: string, value: string): Promise<void> {
  await ctx.browser.evaluate(({ sel, v }: { sel: string; v: string }) => {
    const el = document.querySelector(sel) as HTMLSelectElement | null;
    if (!el) throw new Error(`no ${sel}`);
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(el, v);
    el.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  }, { sel: css, v: value });
  await waitValue(ctx, css, value);
}

export interface Download { name: string; base64: string }

/** Keep the next download in the page instead of saving it (the bytes of the blob the link points at). */
export async function catchDownloads(ctx: Ctx): Promise<void> {
  await ctx.browser.evaluate((_: null) => {
    const w = window as unknown as { __hbBlob?: Blob; __hbName?: string };
    const create = URL.createObjectURL.bind(URL);
    URL.createObjectURL = (b: Blob | MediaSource) => { if (b instanceof Blob) w.__hbBlob = b; return create(b); };
    const click = HTMLAnchorElement.prototype.click;
    HTMLAnchorElement.prototype.click = function (this: HTMLAnchorElement) {
      if (this.download) { w.__hbName = this.download; return; }
      click.call(this);
    };
    return true;
  }, null);
}

/** The file the page last downloaded (after `catchDownloads`). */
export async function takeDownload(ctx: Ctx): Promise<Download> {
  await waitFor(ctx, (_: null) => (window as unknown as { __hbName?: string }).__hbName !== undefined, null, 'the pack file download');
  return ctx.browser.evaluate(async (_: null) => {
    const w = window as unknown as { __hbBlob: Blob; __hbName: string };
    const bytes = new Uint8Array(await w.__hbBlob.arrayBuffer());
    let s = '';
    for (const b of bytes) s += String.fromCharCode(b);
    return { name: w.__hbName, base64: btoa(s) };
  }, null);
}
