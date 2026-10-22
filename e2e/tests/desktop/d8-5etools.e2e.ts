import { existsSync, rmSync, symlinkSync } from 'node:fs';
import { resolve } from 'node:path';
import { desktopSuite, expect, NAME, openAdditionalContent, pressKey, SLOW, settle, typeText, createViaShortcut } from './helpers.js';

// d8: real backend, Additional content > 5etools import on the committed SRD fixture folder (override with
// BOREALIS_5ETOOLS_DIR, e.g. a full local data/ folder: never committed). Scan -> notice (first time tick nothing; second time tick
// "Don't show this again"; third scan has no notice; Settings > Data files brings it back) -> native folder panel ->
// source list (PHB and XPHB) -> Import -> report -> new character picks the imported Fighter and, at level 3, Champion.
const FIXTURE = resolve(import.meta.dirname, '../../../crates/borealis-import-5etools/tests/fixtures/data');
const DATA = process.env['BOREALIS_5ETOOLS_DIR'] ?? FIXTURE;
const LINK = '/tmp/borealis-e2e-5etools'; // short lowercase path: the panel is typed key by key

/** Importing every book of the full corpus takes minutes; the whole suite gets room for it plus the re-scan. */
const IMPORT_TIMEOUT_MS = 900_000;
const SUITE_TIMEOUT_MS = 1_500_000;

desktopSuite('d8', '5etools import: scan, notice, import report, imported class and subclass in a new character', async (ctx) => {
  const { app, screen } = ctx;
  if (!existsSync(DATA)) throw new Error(`5etools data folder not found: ${DATA}`);
  rmSync(LINK, { force: true });
  symlinkSync(DATA, LINK);
  const pick = async () => {
    await settle();
    await pressKey(ctx, 'ControlOrMeta+Shift+g');
    await settle();
    await typeText(ctx, LINK);
    await pressKey(ctx, 'Enter');
    await settle(800);
    await pressKey(ctx, 'Enter');
  };
  // The 5etools section sits below the fold: native taps on off-screen nodes do nothing, so scroll the page first.
  // The a11y tree lists off-screen nodes as visible and a tap on them is silently lost, so scroll to the top, then
  // down one swipe at a time, tapping the button after each until the notice (or the folder panel) answers.
  const webview = screen.getByRole('webview' as never, 'Borealis');
  const scan = async () => {
    const button = screen.getByRole('button', /^Scan folder/);
    for (let i = 0; i < 12; i += 1) await webview.swipe({ direction: 'down' });
    for (let step = 0; step < 14; step += 1) {
      await button.tap();
      try {
        await expect(screen.getByText('Import only content you own')).toBeVisible({ timeout: 1500 });
        return;
      } catch { /* not hit yet */ }
      await webview.swipe({ direction: 'up' });
    }
  };

  await app.open();
  await openAdditionalContent(ctx);
  await expect(screen.getByText(/^5etools import$/i).first()).toBeVisible(SLOW);

  // 1) first scan: the notice appears, nothing ticked.
  await scan();
  await expect(screen.getByText('Import only content you own')).toBeVisible(SLOW);
  await screen.getByRole('button', 'I understand, continue').tap();
  const scanStart = Date.now();
  await pick();
  await expect(screen.getByText(/\(PHB\)$/).first()).toBeVisible({ timeout: 300_000 });
  console.info(`[d8] folder picked to source list visible: ${Date.now() - scanStart} ms`);
  await expect(screen.getByText(/\(XPHB\)$/).first()).toBeVisible(SLOW);

  // 2) second scan: tick "Don't show this again".
  await scan();
  await expect(screen.getByText('Import only content you own')).toBeVisible(SLOW);
  await screen.getByText(/^Don't show this again$/).first().tap();
  await screen.getByRole('button', 'I understand, continue').tap();
  await pick();
  await expect(screen.getByText(/\(PHB\)$/).first()).toBeVisible(SLOW);

  // 3) third scan: no notice, the panel opens straight away.
  await screen.getByRole('button', /^Scan folder/).tap(); // the page is still scrolled to the button
  await settle();
  await expect(screen.getByText('Import only content you own')).toBeHidden({ timeout: 3000 });
  await pick();
  await expect(screen.getByText(/\(PHB\)$/).first()).toBeVisible(SLOW);

  // Import everything found.
  const importStart = Date.now();
  await screen.getByRole('button', /^Import \d+ book/).tap();
  await expect(screen.getByText(/not redistributable/).first()).toBeVisible({ timeout: IMPORT_TIMEOUT_MS });
  console.info(`[d8] import tap to report visible: ${Date.now() - importStart} ms`);
  await expect(screen.getByText(/^0 elements converted/)).toBeHidden({ timeout: 3000 });
  await expect(screen.getByText(/\berror\b/i).first()).toBeHidden({ timeout: 3000 });

  // New character with the imported Fighter (its source label is the 5etools book, not SRD).
  await createViaShortcut(ctx, NAME);
  await screen.getByText('Class', { exact: true }).first().tap();
  await expect(screen.getByRole('checkbox', /^Fighter, (?!SRD)/).first()).toBeVisible(SLOW);
  await screen.getByRole('checkbox', /^Fighter, (?!SRD)/).first().doubleTap();
  await expect(screen.getByText(/\berror\b|failed|does not exist/i)).toBeHidden({ timeout: 5000 });
  const noError = () => expect(screen.getByText(/\berror\b|failed|does not exist/i)).toBeHidden({ timeout: 5000 });

  // Imported species (PHB Dwarf, not the SRD one).
  await screen.getByText('Species', { exact: true }).first().tap();
  await expect(screen.getByRole('checkbox', /^Dwarf, (?!SRD)/).first()).toBeVisible(SLOW);
  await screen.getByRole('checkbox', /^Dwarf, (?!SRD)/).first().doubleTap();
  await noError();

  // Level 3 and the imported Champion subclass.
  await screen.getByRole('button', 'Level Up').tap();
  await expect(screen.getByText(/LEVEL UP & MULTICLASSING/i).first()).toBeVisible(SLOW);
  await screen.getByRole('button', 'Level up Fighter').tap();
  await settle(800);
  await screen.getByRole('button', 'Level up Fighter').tap();
  await expect(screen.getByText(/YOUR CHARACTER IS LEVEL 3/i).first()).toBeVisible(SLOW);
  await noError();
  for (let attempt = 0; attempt < 4; attempt += 1) {
    await screen.getByText('Class', { exact: true }).first().tap();
    try {
      await expect(screen.getByText(/^Fighter Subclass/).first()).toBeVisible({ timeout: 3000 });
      break;
    } catch { /* tab not switched yet */ }
  }
  // At level 3 the imported Champion is the (only) Fighter subclass and gets selected, sourced from the imported book.
  // The fixture has one Champion (selected for us); the full corpus lists many subclasses, so pick the 2024 one.
  const champion = screen.getByRole('checkbox', /^Champion, Player's Handbook \(2024\)/);
  try {
    await expect(champion).toBeVisible({ timeout: 4000 });
    await champion.doubleTap();
  } catch { /* already selected */ }
  await expect(screen.getByRole('button', /^Clear Champion/)).toBeVisible(SLOW);
  await expect(screen.getByText(/^Source\. Player's Handbook/).first()).toBeVisible(SLOW);
  await noError();

  // Notice returns after resetting it in Settings > Data files.
  await screen.getByRole('button', 'Settings').tap();
  await screen.getByText(/^data files$/i).first().tap();
  await screen.getByText(/^Content Import Notice$/).first().tap();
  await screen.getByRole('button', 'Save').nth(1).tap(); // the dialog's Save (the sheet bar has its own)
  await settle();

  // The next scan shows the notice again.
  await screen.getByRole('link', /^START$/).tap();
  await openAdditionalContent(ctx);
  await scan();
  await expect(screen.getByText('Import only content you own')).toBeVisible(SLOW);
  await noError();
}, { timeout: SUITE_TIMEOUT_MS });
