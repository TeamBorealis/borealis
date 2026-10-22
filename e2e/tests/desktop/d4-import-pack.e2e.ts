import { copyFileSync, mkdirSync, rmSync } from 'node:fs';
import { desktopSuite, expect, openAdditionalContent, settle, pressKey, SLOW, typeText } from './helpers.js';

// d4: Content > Additional Content > Import files (native open panel, pointed at a temp file via
// Cmd+Shift+G) -> Import preview (count line, unresolved reference, no AI text) -> Install -> pack is listed.
desktopSuite('d4', 'Content: import an XML pack through preview and install', async (ctx) => {
  const { app, screen } = ctx;
  // A short lowercase path: the engine types it key by key into the open panel.
  const dir = '/tmp/borealis-e2e-import';
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  copyFileSync('fixtures/d4-pack.xml', `${dir}/d4pack.xml`);
  await app.open();
  await openAdditionalContent(ctx);
  await screen.getByRole('button', 'Import files').tap();
  // The content import notice comes first, before the native open panel.
  await screen.getByRole('button', 'I understand, continue').tap();
  await settle();
  await pressKey(ctx, 'ControlOrMeta+Shift+g');
  await settle();
  await typeText(ctx, `${dir}/d4pack.xml`);
  await pressKey(ctx, 'Enter');
  await pressKey(ctx, 'Enter');
  await expect(screen.getByText(/2 elements from 1 file will be added as "d4pack"/)).toBeVisible(SLOW);
  await expect(screen.getByText(/ID_MOONFEN_FEAT_MISSING/)).toBeVisible(SLOW);
  await screen.getByRole('button', 'Install').tap();
  await expect(screen.getByText(/2 elements from 1 file/)).toBeHidden(SLOW);
  // Install ends with the import report dialog; dismiss it, then the pack is listed behind it.
  await expect(screen.getByText('2 elements converted; 0 errors, 0 warnings, 1 note.')).toBeVisible(SLOW);
  await screen.getByRole('button', 'Close').first().tap();
  await expect(screen.getByText('Import report').first()).toBeHidden(SLOW);
  await expect(screen.getByText('d4pack').first()).toBeVisible(SLOW);
});
