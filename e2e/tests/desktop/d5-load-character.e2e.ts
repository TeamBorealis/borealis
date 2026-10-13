import { copyFileSync, mkdirSync, rmSync } from 'node:fs';
import { desktopSuite, expect, pressKey, settle, SLOW, typeText } from './helpers.js';

// d5: File > Load Character opens the backend's native open panel (the webview cannot click a file input
// outside a gesture) -> Cmd+Shift+G to a temp .dnd5e -> import report -> the character is listed.
desktopSuite('d5', 'File > Load Character: native open panel imports a .dnd5e', async (ctx) => {
  const { app, screen } = ctx;
  // A short lowercase path: the engine types it key by key into the open panel.
  const dir = '/tmp/borealis-e2e-load';
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  copyFileSync('fixtures/d5-hero.dnd5e', `${dir}/hero.dnd5e`);
  await app.open();
  // The native macOS menu bar also has a "File" item; open whichever File menu offers Load Character (the in-app one).
  const load = screen.getByRole('menuitem', 'Load Character');
  for (const file of await screen.getByRole('menuitem', 'File').all()) {
    await file.tap();
    if (await load.isVisible()) break;
    await pressKey(ctx, 'Escape');
  }
  await load.tap();
  await settle();
  await pressKey(ctx, 'ControlOrMeta+Shift+g');
  await settle();
  await typeText(ctx, `${dir}/hero.dnd5e`);
  await pressKey(ctx, 'Enter');
  await pressKey(ctx, 'Enter');
  await expect(screen.getByText(/^import report$/i).first()).toBeVisible(SLOW);
  await expect(screen.getByText(/Test Hero/).first()).toBeVisible(SLOW);
  await screen.getByRole('button', 'Done').tap();
  await expect(screen.getByText(/Test Hero/).first()).toBeVisible(SLOW);
});
