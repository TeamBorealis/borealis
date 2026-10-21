import {
  createViaShortcut, desktopSuite, expect, IMPORT_NOTICE, linkFiveToolsData, NAME, openAdditionalContent, pickFiveToolsFolder,
  scanFiveTools, settle, SLOW, tapInView,
} from './helpers.js';

// d9 (B-07): real backend, 5etools import narrowed with the source filters, then 2014 characters built from the
// imported Player's Handbook only. The source list is filtered by name and edition and every other book is left out
// with the group "All" boxes; the report names one pack. A known caster (Sorcerer: "Spell (Sorcerer 1)" slots), a
// spellbook caster (Wizard: "Spellbook (Wizard 1)" slots offering imported spells) and a prerequisite feat (Variant
// Human's feat: War Caster, which needs spellcasting, is offered to the Wizard; Inspiring Leader, which needs
// Charisma 13, is not, at point-buy 8). Needs the full local data folder (BOREALIS_5ETOOLS_DIR): the committed
// fixture has no Sorcerer and no feats with prerequisites. Engine side: src-tauri/tests/fivetools_prereqs.rs.
const PHB = /^Player's Handbook \(PHB\)$/;
const PHB_SOURCE = /, Player's Handbook$/; // the 2014 book; the 2024 one is "Player's Handbook (2024)"

desktopSuite('d9', '5etools PHB only via filters; known caster, spellbook caster and a prerequisite feat', async (ctx) => {
  const { app, screen } = ctx;
  linkFiveToolsData();
  await app.open();
  await openAdditionalContent(ctx);
  await scanFiveTools(ctx);
  await screen.getByRole('button', 'I understand, continue').tap();
  await pickFiveToolsFolder(ctx);
  await expect(screen.getByText(PHB).first()).toBeVisible({ timeout: 300_000 });

  // Leave every book out, then filter down to the 2014 Player's Handbook and take only it.
  for (const group of ['books', 'adventures', 'homebrew & other']) {
    const all = screen.getByRole('checkbox', `Select all ${group}`);
    if ((await all.count()) > 0) await tapInView(ctx, all);
  }
  await expect(screen.getByRole('button', 'Import 0 book(s)')).toBeVisible(SLOW);
  await tapInView(ctx, screen.getByRole('searchbox', 'Filter sources'));
  await screen.getByRole('searchbox', 'Filter sources').fill("player's handbook");
  await expect(screen.getByText(/^Player's Handbook \(2024\) \(XPHB\)$/).first()).toBeVisible(SLOW);
  await screen.getByRole('combobox', 'Edition').tap();
  await screen.getByRole('menuitem', '2014 rules').tap();
  await expect(screen.getByText(/\(XPHB\)$/)).toBeHidden(SLOW);
  await tapInView(ctx, screen.getByText(PHB).first());
  await expect(screen.getByRole('button', 'Import 1 book(s)')).toBeVisible(SLOW);
  await tapInView(ctx, screen.getByRole('button', 'Import 1 book(s)'));
  await expect(screen.getByText(/^\d+ elements converted into 1 pack\(s\)\./).first()).toBeVisible({ timeout: 300_000 });
  await expect(screen.getByText(/not redistributable/).first()).toBeVisible(SLOW);
  await expect(screen.getByText(IMPORT_NOTICE)).toBeHidden({ timeout: 3000 });

  // Known caster: a 2014 PHB Sorcerer learns spells in "Spell (Sorcerer 1)" slots.
  await newCharacter2014(ctx);
  await pickClass(ctx, 'Sorcerer');
  await screen.getByRole('link', /^MAGIC$/).tap();
  const known = screen.getByRole('cell', /^Spell \(Sorcerer 1\) \(1 of 2\)/);
  await tapInView(ctx, known);
  await tapInView(ctx, screen.getByRole('button', 'Magic Missile'), { double: true });
  await expect(screen.getByRole('cell', /^Spell \(Sorcerer 1\) \(2 of 2\)/)).toBeVisible(SLOW);
  await noError(ctx);

  // Spellbook caster and prerequisite feat: a 2014 PHB Variant Human Wizard.
  await newCharacter2014(ctx);
  await pickInBuild(ctx, /^Human, /);
  await pickInBuild(ctx, /^Variant/);
  await expect(screen.getByRole('checkbox', /^Alert, Player's Handbook$/)).toBeVisible(SLOW);
  await expect(screen.getByRole('checkbox', /^Inspiring Leader, /)).toBeHidden({ timeout: 3000 }); // needs Charisma 13
  await expect(screen.getByRole('checkbox', /^War Caster, /)).toBeHidden({ timeout: 3000 }); // no spellcasting yet
  await pickClass(ctx, 'Wizard');
  await screen.getByText('Species', { exact: true }).first().tap();
  await pickInBuild(ctx, /^War Caster, Player's Handbook$/);
  await expect(screen.getByRole('button', /^Clear War Caster/)).toBeVisible(SLOW);
  await noError(ctx);
  await screen.getByRole('link', /^MAGIC$/).tap();
  await tapInView(ctx, screen.getByRole('cell', /^Spellbook \(Wizard 1\) \(1 of 6\)/));
  await expect(screen.getByRole('button', 'Magic Missile')).toBeVisible(SLOW);
  await noError(ctx);
}, process.env['BOREALIS_5ETOOLS_DIR'] === undefined ? { skip: 'BOREALIS_5ETOOLS_DIR is not set (the fixture has no Sorcerer)' } : {});

type Ctx = Parameters<Parameters<typeof desktopSuite>[2]>[0];

const noError = (ctx: Ctx) => expect(ctx.screen.getByText(/\berror\b|failed|does not exist/i)).toBeHidden({ timeout: 5000 });

/** Cmd+N with the 2014 rules edition (the imported Player's Handbook is 2014 content). */
async function newCharacter2014(ctx: Ctx): Promise<void> {
  const { screen } = ctx;
  await createViaShortcut(ctx, NAME, async () => {
    await tapInView(ctx, screen.getByText(/^2024 - Revised rules/).first());
    await settle();
    await screen.getByRole('menuitem', /^2014 - Classic rules/).tap();
    await settle(600);
  });
}

/** Double-taps an option of the open Build picker (scrolled into the window first). */
async function pickInBuild(ctx: Ctx, option: RegExp): Promise<void> {
  await tapInView(ctx, ctx.screen.getByRole('checkbox', option).first(), { double: true });
  await settle(600);
}

async function pickClass(ctx: Ctx, name: string): Promise<void> {
  await ctx.screen.getByText('Class', { exact: true }).first().tap();
  await pickInBuild(ctx, new RegExp(`^${name}${PHB_SOURCE.source}`));
  await noError(ctx);
}
