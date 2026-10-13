import { describe, expect, test, type Role, type TestFixtures } from 'e2e';

export const SLOW = { timeout: 30_000 } as const;
export const NAME = 'Mira Desktop';

/** One suite on the real macOS app. Skipped on web/iOS/Android by `platforms`. Cmd is the modifier (U7a keeps native chrome). */
export function desktopSuite(id: string, title: string, body: (ctx: TestFixtures) => Promise<void>): void {
  describe(id, { tags: ['macos', id], platforms: ['macos'] }, () => {
    test(title, { timeout: 420_000 }, body);
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
export async function createViaShortcut(ctx: Ctx, name: string): Promise<void> {
  const { screen } = ctx;
  await pressKey(ctx, 'ControlOrMeta+n');
  await expect(screen.getByText(/^new character$/i).first()).toBeVisible(SLOW);
  // Flow order: details block first, rules edition after the abilities option.
  for (const label of [/^name$/i, /^starting level$/i, /^abilities generation option$/i, /^rules edition$/i]) {
    await expect(screen.getByText(label).first()).toBeVisible(SLOW);
  }
  await screen.getByRole('textbox', /^name$/i).fill(name);
  await screen.getByRole('button', 'Create character').tap();
  await expect(screen.getByText(/CHOOSE ONE FROM THE 'RACE' SELECTION/)).toBeVisible(SLOW);
}

export { expect };
