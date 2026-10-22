import { createViaShortcut, desktopSuite, expect, NAME, SLOW, settle } from './helpers.js';

// d6: real backend, new 2024 High Elf Wizard (Sage): Build > Sage ability increase shows no "does not exist" pane;
// Magic page: a cantrip slot opens its catalogue and takes a cantrip; a "Spellbook (Wizard 1)" slot opens a catalogue
// that offers Magic Missile (real slot ids; the mock hid both bugs). The catalogue has one table per spell list
// (Wizard, High Elf), so a spell can appear twice.
// Not covered here: picking Magic Missile and preparing it. The catalogue sits below the fold of the 1200x800 window,
// and a native tap on it lands on stale coordinates after the pane scrolls (hits the neighbouring slot or tab). That path is
// covered by ui/src/test/magic-real-slot-ids.test.tsx and src-tauri/tests/magic_slots.rs.
desktopSuite('d6', 'New 2024 Wizard: Sage description, cantrip pick, spellbook slot catalogue', async (ctx) => {
  const { app, screen } = ctx;
  await app.open();
  await createViaShortcut(ctx, NAME);
  await screen.getByRole('checkbox', /^Elf, SRD 5\.2/).doubleTap();
  await screen.getByRole('checkbox', /^High Elf, SRD 5\.2/).doubleTap();
  await screen.getByRole('checkbox', /^Intelligence, SRD 5\.2/).doubleTap();
  await expect(screen.getByText(/CHOOSE ONE FROM THE 'CLASS' SELECTION/)).toBeVisible(SLOW);
  await screen.getByText('Class', { exact: true }).first().tap();
  await screen.getByRole('checkbox', /^Wizard, SRD 5\.2/).doubleTap();
  await screen.getByText('Background', { exact: true }).first().tap();
  await screen.getByRole('checkbox', /^Sage, SRD 5\.2/).doubleTap();
  const sage = screen.getByRole('checkbox', /^Intelligence \+2, Wisdom \+1/);
  await sage.tap();
  await expect(screen.getByText(/does not exist/i)).toBeHidden({ timeout: 5000 });
  await sage.doubleTap();
  await expect(screen.getByText(/does not exist/i)).toBeHidden({ timeout: 5000 });
  await expect(screen.getByRole('button', /^Clear Intelligence \+2, Wisdom \+1/)).toBeVisible(SLOW);
  await screen.getByRole('link', /^MAGIC$/).tap();
  await expect(screen.getByText(/Spellbook \(Wizard 1\)/).first()).toBeVisible(SLOW);
  await screen.getByRole('cell', /^Cantrip \(Wizard 1\) \(1 of 3\)/).tap();
  await expect(screen.getByText('No spells to show.')).toBeHidden(SLOW);
  await screen.getByRole('button', 'Acid Splash').first().doubleTap();
  await expect(screen.getByText('CANTRIPS:1/3')).toBeVisible(SLOW);
  await screen.getByRole('table', 'Selection slots').swipe({ direction: 'up' });
  await settle();
  await screen.getByRole('cell', /^Spellbook \(Wizard 1\) \(1 of 6\)/).tap();
  await expect(screen.getByRole('button', 'Magic Missile').first()).toBeVisible(SLOW);
});
