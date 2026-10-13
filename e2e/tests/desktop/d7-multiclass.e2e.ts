import { desktopSuite, expect, NAME, pressKey, SLOW, settle } from './helpers.js';

// d7: real backend, multiclass flow (regression): a new 2024 Elf Wizard 1 with STR 13 / INT 13 opens
// Level up & multiclassing, presses "New multiclass Fighter", lands on Build > Class with the empty
// "Multiclass (Level 2)" picker (spec: NEW MULTICLASS adds an empty class slot, the class is picked here), which lists
// Fighter (it said "Nothing matches" for every class: engine-derived multiclass entries were dropped by the backend).
// Picking Fighter makes the character Wizard / Fighter. Rust: src-tauri/tests/multiclass_picker.rs; UI: multiclass-real-picker.
const RAISES = 5; // point buy 8 -> 13

desktopSuite('d7', 'New 2024 Wizard: New multiclass Fighter, pick Fighter in the Multiclass picker', async (ctx) => {
  const { app, screen } = ctx;
  await app.open();
  await pressKey(ctx, 'ControlOrMeta+n');
  await expect(screen.getByText(/^abilities generation option$/i).first()).toBeVisible(SLOW);
  await screen.getByText(/^Roll 4d6, drop lowest/).first().tap();
  await settle();
  await screen.getByRole('menuitem').nth(0).tap();
  await settle(600);
  await screen.getByRole('textbox', /^name$/i).fill(NAME);
  await screen.getByRole('button', 'Create character').tap();
  await expect(screen.getByText(/CHOOSE ONE FROM THE 'RACE' SELECTION/)).toBeVisible(SLOW);
  await screen.getByRole('checkbox', /^Elf, SRD 5\.2/).doubleTap();
  await screen.getByRole('checkbox', /^High Elf, SRD 5\.2/).doubleTap();
  await screen.getByRole('checkbox', /^Intelligence, SRD 5\.2/).doubleTap();
  await expect(screen.getByText(/CHOOSE ONE FROM THE 'CLASS' SELECTION/)).toBeVisible(SLOW);
  await screen.getByText('Class', { exact: true }).first().tap();
  await screen.getByRole('checkbox', /^Wizard, SRD 5\.2/).doubleTap();

  // Fighter needs Strength 13 or Dexterity 13; leaving Wizard needs Intelligence 13.
  await screen.getByText('Ability Scores', { exact: true }).first().tap();
  for (const ability of ['Strength', 'Intelligence']) {
    for (let i = 0; i < RAISES; i += 1) {
      await screen.getByRole('button', `Increase ${ability}`).tap();
      await settle(300);
    }
  }

  await screen.getByRole('button', 'Level Up').tap();
  await expect(screen.getByText(/LEVEL UP & MULTICLASSING/i).first()).toBeVisible(SLOW);
  await screen.getByRole('button', 'New multiclass Fighter').tap();

  await expect(screen.getByText(/MULTICLASS \(LEVEL 2\)/i).first()).toBeVisible(SLOW);
  await expect(screen.getByText(/Nothing matches/)).toBeHidden({ timeout: 5000 });
  const fighter = screen.getByRole('checkbox', /^Fighter, SRD 5\.2/);
  await expect(fighter).toBeVisible(SLOW);
  await fighter.doubleTap();
  await expect(screen.getByText(/WIZARD \/ FIGHTER/i).first()).toBeVisible(SLOW);
});
