import {
  createViaShortcut, desktopSuite, expect, linkFiveToolsData, NAME, openAdditionalContent, pickFiveToolsFolder, pickInBuild, pressKey,
  scanFiveTools, SLOW, settle, tapInView,
} from './helpers.js';

// d8: real backend, Additional content > 5etools import on the committed SRD fixture folder (override with
// BOREALIS_5ETOOLS_DIR, e.g. a full local data/ folder: never committed). Scan -> notice (first time tick nothing; second time tick
// "Don't show this again"; third scan has no notice; Settings > Data files brings it back) -> native folder panel ->
// source list (PHB and XPHB) -> Import -> report -> new character picks the imported Fighter and, at level 3, Champion.
const XPHB = "Player's Handbook (2024)"; // the source label of imported XPHB entries

/** Importing all 125 books of the full corpus showed its report 15 s after the tap (B-07 run 2); generous room. */
const IMPORT_TIMEOUT_MS = 300_000;
const SUITE_TIMEOUT_MS = 1_200_000;

desktopSuite('d8', '5etools import: scan, notice, import report, imported class and subclass in a new character', async (ctx) => {
  const { app, screen } = ctx;
  linkFiveToolsData();
  // The folder path is pasted: typed key by key the panel lost keys and, once, the window (B-07 run 2).
  const pick = () => pickFiveToolsFolder(ctx);
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

  const report = () => screen.getByText(/not redistributable/).first();
  const startImport = async () => {
    const button = screen.getByRole('button', /^Import \d+ book/);
    for (let attempt = 0; attempt < 4; attempt += 1) {
      await tapInView(ctx, button);
      for (const answer of [screen.getByRole('progressbar', 'Import progress'), report()]) {
        try {
          await expect(answer).toBeVisible({ timeout: 5000 });
          return;
        } catch { /* not this one */ }
      }
    }
    throw new Error('the Import button never started an import');
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
  // The button heads the source list, below the fold after the scans: a tap on it there is silently lost (the
  // import never starts), so scroll it into the window and tap until the progress bar (or the report) answers.
  const importStart = Date.now();
  await startImport();
  await expect(report()).toBeVisible({ timeout: IMPORT_TIMEOUT_MS });
  console.info(`[d8] import tap to report visible: ${Date.now() - importStart} ms`);
  await expect(screen.getByText(/^0 elements converted/)).toBeHidden({ timeout: 3000 });
  await expect(screen.getByText(/\berror\b/i).first()).toBeHidden({ timeout: 3000 });

  // New (2024 rules) character with the imported Fighter (its source label is the 5etools book, not SRD).
  await createViaShortcut(ctx, NAME);
  await screen.getByText('Class', { exact: true }).first().tap();
  await pickInBuild(ctx, 'Fighter', XPHB);
  await expect(screen.getByText(/\berror\b|failed|does not exist/i)).toBeHidden({ timeout: 5000 });
  const noError = () => expect(screen.getByText(/\berror\b|failed|does not exist/i)).toBeHidden({ timeout: 5000 });

  // Imported species (the 2024 Player's Handbook Dwarf, not the SRD one).
  await screen.getByText('Species', { exact: true }).first().tap();
  await pickInBuild(ctx, 'Dwarf', XPHB);
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
    await tapInView(ctx, champion, { double: true });
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

  // The next scan shows the notice again. Every imported book is now an installed pack listed above the 5etools
  // section (125 rows on the full corpus): too far for swipes, so click plain text on the page (WebKit then scrolls
  // that area from the keyboard) and jump to its end, where "Scan folder..." is a few swipes up.
  await screen.getByRole('link', /^START$/).tap();
  await openAdditionalContent(ctx);
  await screen.getByText(/^Import XML files, /).first().tap();
  await pressKey(ctx, 'End');
  await settle(800);
  await scanFiveTools(ctx);
  await noError();
}, { timeout: SUITE_TIMEOUT_MS });
