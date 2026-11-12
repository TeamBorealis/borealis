import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import type { TestFixtures } from 'e2e';
import { createViaShortcut, desktopSuite, expect, NAME, pressKey, settle, SLOW, tapInView, typeText } from './helpers.js';

// d12 (homebrew creator, spec 18.4 flow 1 through the real macOS window): with the creator switched on in settings.json,
// START > HOMEBREW > New pack > New element (a spell): 1st level, Evocation (the native selects, chosen by typing as a
// user does), on the Wizard list; Cmd+S saves it into the pack and reloads content. Then Cmd+N makes a 2024 High Elf
// Wizard, and the MAGIC page's spellbook slot offers the new spell next to the SRD's. The rest of flow 1 (taking it at
// level 3, slots, casting) is src-tauri/tests/homebrew_flows.rs; the other flows' real-window halves are there too.
// The engine backs up the user's ~/Library data and gives each attempt an empty data folder (engines/macos).

const SPELL = 'Ember Bolt';
const PACK = 'Ember Isles';

/** The app's settings file in the test's (empty, backed-up) data folder, with the creator switched on. */
function enableCreator(): void {
  const dir = join(homedir(), 'Library/Application Support/app.borealis');
  const file = join(dir, 'settings.json');
  mkdirSync(dir, { recursive: true });
  const current = existsSync(file) ? (JSON.parse(readFileSync(file, 'utf8')) as Record<string, unknown>) : {};
  writeFileSync(file, JSON.stringify({ ...current, creatorEnabled: true }));
}

/**
 * A native select, chosen as a keyboard user does: a click on its label focuses it (the macOS tree lists a select with
 * no value yet under no name, so the label is what can be found), typing picks the first option that starts so.
 */
async function chooseByLabel(ctx: TestFixtures, label: string, typed: string): Promise<void> {
  await ctx.screen.getByText(label, { exact: true }).last().tap();
  await settle(800);
  await typeText(ctx, typed);
  await settle(800);
}

desktopSuite('d12', 'Creator: a spell made in Homebrew is offered by a new Wizard spellbook (real window)', async (ctx) => {
  const { app, screen } = ctx;
  await app.open();
  enableCreator();
  await app.restart();
  await expect(screen.getByText(/welcome to borealis/i).first()).toBeVisible(SLOW);

  // START > HOMEBREW: the first click after a launch can be swallowed by window activation, so tap until it shows.
  const tab = screen.getByText(/^homebrew$/i).first();
  await expect(tab).toBeVisible(SLOW);
  for (let attempt = 0; attempt < 4; attempt += 1) {
    await tab.tap();
    try { await expect(screen.getByRole('button', 'New pack').first()).toBeVisible({ timeout: 4000 }); break; } catch { /* again */ }
  }
  await screen.getByRole('button', 'New pack').first().tap();
  await expect(screen.getByText(/^new pack$/i).first()).toBeVisible(SLOW);
  await screen.getByPlaceholder('My homebrew').fill(PACK);
  await screen.getByRole('button', 'Create').tap();
  // the pack's workspace: its (empty) element list offers New element
  await expect(screen.getByText('No spells in this pack yet.').first()).toBeVisible(SLOW);

  // New element: a spell (the first kind), named.
  await screen.getByRole('button', 'New element').first().tap();
  await expect(screen.getByText(/^new element$/i).first()).toBeVisible(SLOW);
  await screen.getByPlaceholder('Unnamed').fill(SPELL);
  await screen.getByRole('button', 'Create').tap();
  await expect(screen.getByRole('button', /Problems \(1\)/)).toBeVisible(SLOW);
  // a new spell is 1st level; the school is the one thing it still needs
  await chooseByLabel(ctx, 'SCHOOL', 'evo');
  await expect(screen.getByText('A spell needs a school.')).toBeHidden(SLOW);
  // below the fold of the 1200x800 window: scrolled into view first (a click outside the window is lost)
  await tapInView(ctx, screen.getByRole('checkbox', /^wizard$/i).first());
  await expect(screen.getByRole('button', /Problems \(0\)/)).toBeVisible(SLOW);
  await settle();
  await pressKey(ctx, 'ControlOrMeta+s');
  // saved into the pack: CATEGORIES counts it (the status line's "Saved ..." gives way to the problems line at once)
  await expect(screen.getByRole('button', 'Spells (1)')).toBeVisible(SLOW);

  // A new 2024 High Elf Wizard: its spellbook slot offers the homebrew spell (content reloaded by the save).
  await createViaShortcut(ctx, NAME);
  await screen.getByRole('checkbox', /^Elf, SRD 5\.2/).doubleTap();
  await screen.getByRole('checkbox', /^High Elf, SRD 5\.2/).doubleTap();
  await screen.getByRole('checkbox', /^Intelligence, SRD 5\.2/).doubleTap();
  await expect(screen.getByText(/CHOOSE ONE FROM THE 'CLASS' SELECTION/)).toBeVisible(SLOW);
  await screen.getByText('Class', { exact: true }).first().tap();
  await screen.getByRole('checkbox', /^Wizard, SRD 5\.2/).doubleTap();
  await screen.getByRole('link', /^MAGIC$/).tap();
  await expect(screen.getByText(/Spellbook \(Wizard 1\)/).first()).toBeVisible(SLOW);
  await screen.getByRole('table', 'Selection slots').swipe({ direction: 'up' });
  await settle();
  await screen.getByRole('cell', /^Spellbook \(Wizard 1\) \(1 of 6\)/).tap();
  await expect(screen.getByRole('button', 'Magic Missile').first()).toBeVisible(SLOW);
  await expect(screen.getByRole('button', SPELL).first()).toBeVisible(SLOW);
});
