import { execFileSync } from 'node:child_process';
import { existsSync, rmSync, symlinkSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, type Locator, type Role, type TestFixtures } from 'e2e';

export const SLOW = { timeout: 30_000 } as const;
export const NAME = 'Mira Desktop';

/** One suite on the real macOS app. Skipped on web/iOS/Android by `platforms`. Cmd is the modifier (U7a keeps native chrome). */
export function desktopSuite(
  id: string, title: string, body: (ctx: TestFixtures) => Promise<void>, opts: { timeout?: number; skip?: string } = {},
): void {
  describe(id, { tags: ['macos', id], platforms: ['macos'] }, () => {
    test(title, { timeout: opts.timeout ?? 420_000, ...(opts.skip === undefined ? {} : { skip: opts.skip }) }, body);
  });
}

type Ctx = TestFixtures;

/** Sends a key or chord to the app (the webview holds focus); the engine does not click for `press`. */
/** Let a native panel/sheet finish animating before keystrokes (typing too early drops characters). */
export const settle = (ms = 1500): Promise<void> => new Promise((r) => setTimeout(r, ms));

/** The macOS engine's native web view role (not in the cross-platform `Role` union). */
const WEBVIEW = 'webview' as Role;

export async function pressKey(ctx: Ctx, key: string): Promise<void> {
  await ctx.screen.getByRole(WEBVIEW, 'Borealis').press(key);
}

/** Types text into whatever holds focus (e.g. a native panel) one key at a time; US-layout characters only. */
export async function typeText(ctx: Ctx, text: string): Promise<void> {
  for (const ch of text) await pressKey(ctx, ch === ' ' ? 'Space' : ch);
}

/** Opens Start > Additional content. The first click after a launch can be swallowed by window activation, so tap until the page shows. */
export async function openAdditionalContent(ctx: Ctx): Promise<void> {
  const { screen } = ctx;
  const tab = screen.getByText(/^additional content$/i).first();
  await expect(tab).toBeVisible(SLOW);
  for (let attempt = 0; attempt < 4; attempt += 1) {
    await tab.tap();
    try {
      await expect(screen.getByText(/^custom content$/i).first()).toBeVisible({ timeout: 4000 });
      return;
    } catch { /* tab not switched yet: tap again */ }
  }
  await expect(screen.getByText(/^custom content$/i).first()).toBeVisible(SLOW);
}

/** Cmd+N -> New character flyout (edition, name, level, method, options order) -> name -> Create character. */
export async function createViaShortcut(ctx: Ctx, name: string, beforeCreate?: () => Promise<void>): Promise<void> {
  const { screen } = ctx;
  await pressKey(ctx, 'ControlOrMeta+n');
  await expect(screen.getByText(/^new character$/i).first()).toBeVisible(SLOW);
  // Flow order: details block first, rules edition after the abilities option.
  for (const label of [/^name$/i, /^starting level$/i, /^abilities generation option$/i, /^rules edition$/i]) {
    await expect(screen.getByText(label).first()).toBeVisible(SLOW);
  }
  await screen.getByRole('textbox', /^name$/i).fill(name);
  if (beforeCreate) await beforeCreate();
  await screen.getByRole('button', 'Create character').tap();
  await expect(screen.getByText(/CHOOSE ONE FROM THE 'RACE' SELECTION/)).toBeVisible(SLOW);
}

/** Most swipes needed to bring a node into the window (each swipe scrolls a few rows). */
const MAX_SCROLLS = 60;
/** A node this close to the window edge still counts as off-screen (its click could land on the frame). */
const EDGE_PX = 24;

const appView = (ctx: Ctx): Locator => ctx.screen.getByRole(WEBVIEW, 'Borealis');

/**
 * Taps `target` after scrolling it inside the Borealis window. The accessibility tree also lists nodes below the
 * fold, and a native click there is silently lost, so the node's rectangle decides which way to scroll.
 */
export async function tapInView(ctx: Ctx, target: Locator, opts: { double?: boolean } = {}): Promise<void> {
  await expect(target).toBeVisible(SLOW);
  for (let i = 0; i < MAX_SCROLLS; i += 1) {
    // The tree can be empty for a moment right after a swipe: look again instead of failing.
    const [box, view] = await Promise.all([target.boundingBox().catch(() => null), appView(ctx).boundingBox().catch(() => null)]);
    if (box === null || view === null) {
      await settle(500);
      continue;
    }
    const mid = box.y + box.height / 2;
    if (mid > view.y + view.height - EDGE_PX) await appView(ctx).swipe({ direction: 'up' });
    else if (mid < view.y + EDGE_PX) await appView(ctx).swipe({ direction: 'down' });
    else break;
    await settle(300);
  }
  await (opts.double === true ? target.doubleTap() : target.tap());
}

/** Short lowercase path to the 5etools data folder (the native panel is typed key by key). */
export const FIVETOOLS_LINK = '/tmp/borealis-e2e-5etools';
export const IMPORT_NOTICE = 'Import only content you own';
/** The committed SRD-safe 5etools fixture folder; BOREALIS_5ETOOLS_DIR (a full local data/ folder, never committed) overrides it. */
const FIVETOOLS_FIXTURE = resolve(import.meta.dirname, '../../../crates/borealis-import-5etools/tests/fixtures/data');

/** Points FIVETOOLS_LINK at the 5etools data folder for this run; returns the folder. */
export function linkFiveToolsData(dir: string = process.env['BOREALIS_5ETOOLS_DIR'] ?? FIVETOOLS_FIXTURE): string {
  if (!existsSync(dir)) throw new Error(`5etools data folder not found: ${dir}`);
  rmSync(FIVETOOLS_LINK, { force: true });
  symlinkSync(dir, FIVETOOLS_LINK);
  return dir;
}

/** Types the 5etools folder into the native open panel (Go to folder) and opens it. */
export async function pickFiveToolsFolder(ctx: Ctx): Promise<void> {
  await settle();
  await pressKey(ctx, 'ControlOrMeta+Shift+g');
  await settle();
  await pasteText(ctx, FIVETOOLS_LINK);
  await pressKey(ctx, 'Enter');
  await settle(800);
  await pressKey(ctx, 'Enter');
}

/** Pastes text with Cmd+V (typing key by key takes ~1 s a key and can lose the panel); the user's clipboard is put back. */
export async function pasteText(ctx: Ctx, text: string): Promise<void> {
  const previous = execFileSync('pbpaste', { encoding: 'utf8' });
  try {
    execFileSync('pbcopy', { input: text });
    await pressKey(ctx, 'ControlOrMeta+v');
    await settle(800);
  } finally {
    execFileSync('pbcopy', { input: previous });
  }
}

/**
 * Taps "Scan folder..." (below the fold of Additional content) and waits for the notice. A tap the page swallows
 * (window activation) is repeated until the notice answers.
 */
export async function scanFiveTools(ctx: Ctx): Promise<void> {
  const { screen } = ctx;
  const button = screen.getByRole('button', /^Scan folder/);
  for (let attempt = 0; attempt < 3; attempt += 1) {
    await tapInView(ctx, button);
    try {
      await expect(screen.getByText(IMPORT_NOTICE)).toBeVisible({ timeout: 3000 });
      return;
    } catch { /* tap lost: try again */ }
  }
  await expect(screen.getByText(IMPORT_NOTICE)).toBeVisible(SLOW);
}

export { expect };
