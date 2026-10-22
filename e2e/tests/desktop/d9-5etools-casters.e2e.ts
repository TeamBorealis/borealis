import {
  createViaShortcut, desktopSuite, pressKey, expect, IMPORT_NOTICE, linkFiveToolsData, NAME, openAdditionalContent, pickFiveToolsFolder,
  pickInBuild as pickFromBuild, scanFiveTools, settle, SLOW, tapInView,
} from './helpers.js';

// d9 (B-07): real backend, 5etools import narrowed with the source filters, then 2014 characters built from the
// imported Player's Handbook only. The source list is filtered by name and edition and every other book is left out
// with the group "All" boxes; the report names one pack. A known caster (Sorcerer: "Spell (Sorcerer 1)" slots), a
// spellbook caster (Wizard: "Spellbook (Wizard 1)" slots offering imported spells) and a prerequisite feat (Variant
// Human's feat: War Caster, which needs spellcasting, is offered to the Wizard; Inspiring Leader, which needs
// Charisma 13, is not, at point-buy 8). Needs the full local data folder (BOREALIS_5ETOOLS_DIR): the committed
// fixture has no Sorcerer and no feats with prerequisites. Engine side: src-tauri/tests/fivetools_prereqs.rs.
const PHB = /^Player's Handbook \(2014\) \(PHB\)$/;
const PHB_SOURCE = "Player's Handbook (2014)"; // the 2024 book is "Player's Handbook (2024)"

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
  await screen.getByRole('searchbox', 'Filter sources').fill('handbook'); // no apostrophe: macOS text entry turns it into a typographic one
  await expect(screen.getByText(/^Player's Handbook \(2024\) \(XPHB\)$/).first()).toBeVisible(SLOW);
  // The native popup shows in the tree as an unnamed-role node called "Edition" (no combobox role).
  await screen.getByText('Edition', { exact: true }).first().tap();
  // The popup items carry no names, only a test id: All editions, 2014 rules, 2024 rules.
  await pickPopupItem(ctx, 1);
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
  await tapInView(ctx, screen.getByRole('button', 'Magic Missile').first(), { double: true }); // listed twice: the SRD 5.1 and the imported PHB spell
  await expect(screen.getByRole('cell', /^Spell \(Sorcerer 1\) \(2 of 2\)/)).toBeVisible(SLOW);
  await noError(ctx);

  // Spellbook caster and prerequisite feat: a 2014 PHB Variant Human Wizard.
  await newCharacter2014(ctx);
  await pickInBuild(ctx, 'Human');
  await pickInBuild(ctx, 'Human (Variant)');
  // The flow asks for the variant's ability increases first (not Charisma: Inspiring Leader must stay out of reach).
  await pickInBuild(ctx, 'Intelligence +1, Wisdom +1');
  await showFeat(ctx, screen.getByRole('checkbox', /^Alert, Player's Handbook \(2014\)$/));
  await expect(screen.getByRole('checkbox', /^Inspiring Leader, /)).toBeHidden({ timeout: 3000 }); // needs Charisma 13
  await expect(screen.getByRole('checkbox', /^War Caster, /)).toBeHidden({ timeout: 3000 }); // no spellcasting yet
  await pickClass(ctx, 'Wizard');
  await showFeat(ctx, screen.getByRole('checkbox', /^War Caster, Player's Handbook \(2014\)$/));
  await pickInBuild(ctx, 'War Caster');
  await noError(ctx);
  await screen.getByRole('link', /^MAGIC$/).tap();
  await tapInView(ctx, screen.getByRole('cell', /^Spellbook \(Wizard 1\) \(1 of 6\)/));
  await expect(screen.getByRole('button', 'Magic Missile').first()).toBeVisible(SLOW);
  await noError(ctx);
}, process.env['BOREALIS_5ETOOLS_DIR'] === undefined ? { skip: 'BOREALIS_5ETOOLS_DIR is not set (the fixture has no Sorcerer)' } : {});

type Ctx = Parameters<Parameters<typeof desktopSuite>[2]>[0];

const noError = (ctx: Ctx) => expect(ctx.screen.getByText(/\berror\b|failed|does not exist/i)).toBeHidden({ timeout: 5000 });

/**
 * Cmd+N with the 2014 rules edition (the imported Player's Handbook is 2014 content). The edition select is the last
 * row of the flyout's inner scrolling list, clipped at this window size; a swipe scrolls the page, not that list, and
 * WebKit's Tab skips selects unless macOS keyboard navigation is on. So collapse "Character details" (as a user would)
 * and the options list fits.
 */
async function newCharacter2014(ctx: Ctx): Promise<void> {
  const { screen } = ctx;
  await createViaShortcut(ctx, NAME, async () => {
    await screen.getByRole('button', /^character details$/i).first().tap();
    await expect(screen.getByRole('textbox', /^name$/i)).toBeHidden(SLOW);
    await tapInView(ctx, screen.getByText(/^2024 - Revised rules/).first()); // opens the select's pop-up menu
    await pickPopupItem(ctx, 1); // the items are unnamed: 2024 Revised, 2014 Classic
    await expect(screen.getByText(/^2014 - Classic rules/).first()).toBeVisible(SLOW);
  });
}

/**
 * Opens the Build "Feats" tab (the variant's feat slot lives there, not on the species tab) and waits for `option`;
 * if the feat picker is collapsed, its header opens it.
 */
async function showFeat(ctx: Ctx, option: ReturnType<Ctx['screen']['getByRole']>): Promise<void> {
  await ctx.screen.getByText('Feats', { exact: true }).first().tap();
  try {
    await expect(option).toBeVisible({ timeout: 5000 });
  } catch {
    await ctx.screen.getByRole('button', /^FEAT\b/i).first().tap();
    await expect(option).toBeVisible(SLOW);
  }
}

/** Chooses `name` from the imported 2014 Player's Handbook in the open Build picker. */
const pickInBuild = (ctx: Ctx, name: string): Promise<void> => pickFromBuild(ctx, name, PHB_SOURCE);

async function pickClass(ctx: Ctx, name: string): Promise<void> {
  await ctx.screen.getByText('Class', { exact: true }).first().tap();
  await pickInBuild(ctx, name);
  await noError(ctx);
}

/**
 * Chooses the item at `index` of the pop-up menu that was just opened. The items are not in the accessibility tree
 * while it is open and Return would also submit the form behind it, so: Down keys from the first item, then Space.
 */
async function pickPopupItem(ctx: Ctx, index: number): Promise<void> {
  await settle(600);
  for (let i = 0; i < index; i += 1) await pressKey(ctx, 'ArrowDown');
  await pressKey(ctx, 'Space');
  await settle(600);
}
