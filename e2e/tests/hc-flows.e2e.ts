import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { clickCss, dblClickCss, statusMsg, textOf, waitFor, type Ctx } from '../support/flows-a.js';
import {
  addRule, autosaved, catchDownloads, chooseIn, doneChild, kindCount, LOAD_MS, NAME, nameChild, newElement, newPack, pickRow, reveal, save, setNumber, takeDownload, trialLevel, waitValue,
} from '../support/creator.js';

// Homebrew WB-27 (spec 18.4): the ten end-to-end flows on the desktop web target (1355x847) with the mock and the
// creator switched on (`?creator=1`). Each test runs one flow as the spec words it, as far as the mock models it: the
// mock keeps homebrew in the creator (pickers, Try it, export/import), it never hands a saved element to a character
// on the Build pages. Those halves (a character takes the element, casts, levels to 20, equips) run on the real backend
// in src-tauri/tests/homebrew_flows.rs, and flow 1 also through the real macOS window (tests/desktop/d12-creator-spell);
// the phone half (flow 10) is the device suite (scripts/devices, docs/creator-device-e2e.md). Deterministic: no model.

const PACK = 'Ember Isles';
const START = '/?creator=1#/homebrew';

const openTryIt = async (ctx: Ctx): Promise<void> => {
  await ctx.screen.getByRole('button', 'Try it').tap();
  await expect(ctx.screen.getByRole('dialog', 'Try it')).toBeVisible();
};

const closeTryIt = async (ctx: Ctx): Promise<void> => {
  await ctx.screen.getByRole('button', 'Close').last().tap();
  await expect(ctx.screen.getByRole('dialog', 'Try it')).toBeHidden();
};

/** Try it's LEVEL LADDER: a click on level `n`'s row steps the test character there. */
async function ladderTo(ctx: Ctx, n: number): Promise<void> {
  const row = `.hb-try-ladder tr[data-level="${n}"] button.ap-name`;
  await reveal(ctx, row);
  await clickCss(ctx, row);
  await trialLevel(ctx, n);
}

describe('hc-flows homebrew creator end-to-end flows, spec 18.4 (web mock)', { tags: ['homebrew'] }, () => {
  test('flow 1: a 2024 spell, saved, is offered with its level and school by a spell picker and tried on a Wizard', async (ctx) => {
    await bootToHome(ctx, START);
    await newPack(ctx, PACK);
    await newElement(ctx, 'Ember Lance');
    await ctx.browser.locator('#hb-f-level-input').selectOption('2nd level');
    await ctx.browser.locator('#hb-f-school-input').selectOption('Evocation');
    await ctx.browser.locator('[data-field="spellLists"] input[aria-label="Wizard"]').check();
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await save(ctx, 'Ember Lance', PACK);
    await kindCount(ctx, /Spells \(1\)/);

    // Try it: the test character learns it through its class list at the level that casts it.
    await openTryIt(ctx);
    await trialLevel(ctx, 1);
    expectEqual('the draft in its slot', 'Ember Lance', await textOf(ctx, '.hb-try-draft-name'));
    await closeTryIt(ctx);

    // Offered: a spell picker (the Magic grid's columns) lists it at level 2 with its school and the pack.
    await newElement(ctx, 'Ember Ward');
    await ctx.browser.locator('#hb-f-school-input').selectOption('Abjuration');
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await addRule(ctx, 'Gives a spell');
    await ctx.screen.getByRole('button', 'Pick: Spell').tap();
    await expect(ctx.screen.getByRole('table', 'Spells')).toBeVisible();
    await clickCss(ctx, '.hb-spell-picker .sp-levels button:nth-child(3)');
    const choose = '.hb-spell-picker input[aria-label="Choose Ember Lance"]';
    await waitFor(ctx, (sel: string) => document.querySelector(sel) !== null, choose, 'Ember Lance offered at level 2');
    const row = '.hb-spell-picker tr:has(input[aria-label="Choose Ember Lance"])';
    expectEqual('level and school', 'Level 2 · Evocation', await textOf(ctx, `${row} .sp-lvl`));
    expectEqual('source', PACK, await textOf(ctx, `${row} .sp-src`));
  });

  test('flow 2: a feat with a prerequisite, an ability increase and a proficiency, tried at level 4', async (ctx) => {
    await bootToHome(ctx, START);
    await newPack(ctx, PACK);
    await newElement(ctx, 'Ember Heart', /^Feat/);
    await ctx.browser.locator('#hb-f-category-input').fill('General');
    for (const [n, row] of [[1, 'Character level at least'], [2, 'Ability score at least']] as const) {
      await ctx.screen.getByRole('button', 'Add condition').tap();
      const select = `[data-field="requirements"] select[aria-label="Prerequisite, part ${n}: test"]`;
      await waitFor(ctx, (sel: string) => document.querySelector(sel) !== null, select, `prerequisite row ${n}`);
      await ctx.browser.locator(select).selectOption(row);
    }
    await waitFor(ctx, (_: null) => /Level 4\+, Strength 13\+/.test(document.querySelector('.hb-ws-desc')?.textContent ?? ''), null, 'prerequisite in the description');

    // RULES: Strength +1, and a skill proficiency given outright.
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await addRule(ctx, 'Changes a number');
    await ctx.screen.getByRole('button', 'Pick: Statistic').tap();
    await ctx.browser.locator('input[aria-label="Search statistics"]').fill('strength');
    await ctx.screen.getByRole('button', /^Strength( score)?$/).first().tap();
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', /Changes a number \(Strength/)).toBeVisible();
    await addRule(ctx, 'Gives ...');
    await ctx.screen.getByRole('button', 'Pick: What it gives').tap();
    await expect(ctx.screen.getByRole('dialog', /^Choose/)).toBeVisible();
    // the search matches names, as the backend's picker does (Hlw0Y): only the Athletics skill is left
    await ctx.browser.locator('input[aria-label="Search elements"]').fill('athletics');
    await ctx.screen.getByRole('button', /^Athletics,/).tap();
    await ctx.screen.getByRole('button', 'Choose').tap();
    await expect(ctx.screen.getByRole('dialog', /^Choose/)).toBeHidden();
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', /Gives .*Athletics/)).toBeVisible();
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await save(ctx, 'Ember Heart', PACK);

    // Try it: the feat waits for the first feat slot (level 4) and arrives there.
    await openTryIt(ctx);
    await trialLevel(ctx, 1);
    await expect(ctx.screen.getByText(/first feat slot/)).toBeVisible();
    await expect(ctx.screen.getByRole('table', 'Level ladder')).toBeVisible();
    await ladderTo(ctx, 4);
    await expect(ctx.screen.getByText(/Taken at level 4/)).toBeVisible();
    await waitFor(ctx, (_: null) => [...document.querySelectorAll('.hb-try [aria-label="Features"] .hb-try-changed')].some((n) => /Ember Heart/.test(n.textContent ?? '')), null, 'the feat marked at 4');
    await closeTryIt(ctx);
  });
  test('flow 3: a half-caster class with a subclass at 3, ASIs and starting equipment; its Try it ladder is clean to 20', async (ctx) => {
    await bootToHome(ctx, START);
    await newPack(ctx, PACK);
    await newElement(ctx, 'Gloom Knight', /^Class A class/);
    await ctx.browser.locator('#hb-f-hitDie-input').selectOption('d10');

    // LEVELS: the subclass choice at 3 and the ability score improvements from 4 come with a new class.
    await ctx.screen.getByRole('tab', 'LEVELS').tap();
    await expect(ctx.screen.getByRole('table', 'Features by level')).toBeVisible();
    for (const [level, name] of [[3, 'Subclass'], [4, 'Ability Score Improvement']] as const) {
      await waitFor(ctx, ({ n, want }: { n: number; want: string }) =>
        [...document.querySelectorAll(`table[aria-label="Features by level"] tr[data-level="${n}"] .hb-feature-name`)].some((b) => b.textContent === want),
      { n: level, want: name }, `${name} at ${level}`);
    }

    // SPELLCASTING: a half caster (round up), Wisdom, the Ranger list: 2 slots at 1, 4 + 2 at 5.
    await ctx.screen.getByRole('tab', 'SPELLCASTING').tap();
    await ctx.screen.getByRole('button', 'Add spellcasting').tap();
    await ctx.browser.locator('[data-field="casterType"] select').selectOption('Half caster (round up)');
    await ctx.browser.locator('[data-field="ability"] select').selectOption('Wisdom');
    await ctx.browser.locator('[data-field="spellList"] input').fill('Ranger');
    await waitFor(ctx, (_: null) =>
      [...document.querySelectorAll('table[aria-label="Spell slots and spells by level"] tr[data-level="5"] .hb-slot-cell')].map((c) => c.textContent).join(',') === '4,2,-,-,-',
    null, 'half-caster slots at 5');

    // EQUIPMENT: the class's Equipment Package.
    await ctx.screen.getByRole('tab', 'EQUIPMENT').tap();
    await ctx.screen.getByRole('button', 'Add starting equipment').tap();
    await waitValue(ctx, '#hb-package-name', 'Starting Equipment (Gloom Knight)');
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await save(ctx, 'Gloom Knight', PACK);
    await kindCount(ctx, /Classes \(1\)/);

    // A subclass of it, chosen at the class's subclass level.
    await newElement(ctx, 'Ash Order', /^Subclass/);
    await clickCss(ctx, '[data-field="parentClass"] .hb-pick-btn');
    await expect(ctx.screen.getByRole('dialog', /^Choose: Class/)).toBeVisible();
    await pickRow(ctx, 'Gloom Knight');
    await waitFor(ctx, (_: null) => document.querySelector('[data-field="subclassTag"] .hb-readonly')?.textContent === 'Gloom Knight Subclass', null, 'the subclass tag');
    expectEqual('chosen at level', '3', await textOf(ctx, '[data-field="levelGained"] .hb-readonly'));
    await save(ctx, 'Ash Order', PACK);
    await kindCount(ctx, /Subclasses \(1\)/);

    // Try it on the class: the 1-20 ladder has no problems; step to 20, then try it as a second class after Fighter 1.
    await ctx.screen.getByRole('button', 'Classes (1)').tap();
    await ctx.screen.getByRole('button', 'Gloom Knight').tap();
    await waitValue(ctx, NAME, 'Gloom Knight');
    await openTryIt(ctx);
    await trialLevel(ctx, 1);
    await expect(ctx.screen.getByRole('table', 'Level ladder')).toBeVisible();
    await waitFor(ctx, (_: null) => document.querySelectorAll('.hb-try-ladder tr[data-level]').length === 20, null, '20 ladder rows');
    expectEqual('ladder problems', 0, await ctx.browser.evaluate((_: null) => document.querySelectorAll('.hb-try-ladder .hb-try-problem').length, null));
    await ladderTo(ctx, 20);
    await reveal(ctx, '.hb-try-setup input[type="checkbox"]');
    await ctx.browser.locator('.hb-try-setup input[type="checkbox"]').check();
    await expect(ctx.screen.getByText(/Fighter 1, then this class/)).toBeVisible();
    await expect(ctx.screen.getByText('The test character starts as a Fighter, then takes levels in this class.')).toBeVisible();
    await closeTryIt(ctx);
  });

  test('flow 4: a species with a lineage choice and darkvision; a 2014 race with a subrace', async (ctx) => {
    await bootToHome(ctx, START);
    await newPack(ctx, PACK, 'Both editions');
    await newElement(ctx, 'Emberkin', /^Species/);
    await setNumber(ctx, 'darkvision', 60);
    await ctx.screen.getByRole('button', 'Add a racial trait').tap();
    await expect(ctx.screen.getByRole('dialog', /^Racial trait:/)).toBeVisible();
    await nameChild(ctx, 'n1', 'Ember Lineage');
    for (const [key, name] of [['n2', 'Ash'], ['n3', 'Cinder']] as const) {
      await ctx.screen.getByRole('button', 'Add a species trait option').tap();
      await expect(ctx.screen.getByRole('dialog', /^Species trait option:/)).toBeVisible();
      await nameChild(ctx, key, name);
      await doneChild(ctx, key);
    }
    await expect(ctx.screen.getByRole('group', 'Lineage options')).toContainText('Cinder');
    await doneChild(ctx, 'n1');
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await save(ctx, 'Emberkin', PACK);
    // saved and opened again: the darkvision range and the lineage's two options
    await waitValue(ctx, '[data-field="darkvision"] input[role="spinbutton"]', '60');
    await ctx.screen.getByRole('button', 'Ember Lineage').tap();
    await expect(ctx.screen.getByRole('group', 'Lineage options')).toContainText('Ash');
    await expect(ctx.screen.getByRole('group', 'Lineage options')).toContainText('Cinder');
    await doneChild(ctx, 'n1');

    // Try it: the test character is an Emberkin and its lineage is a choice with both options.
    await openTryIt(ctx);
    await trialLevel(ctx, 1);
    expectEqual('the draft in its slot', 'Emberkin', await textOf(ctx, '.hb-try-draft-name'));
    await closeTryIt(ctx);

    // 2014: a race and its subrace (the parent race named), each with an ability increase.
    await newElement(ctx, 'Stonefolk', /^Race A 2014/);
    await setNumber(ctx, 'constitution', 2);
    await save(ctx, 'Stonefolk', PACK);
    await newElement(ctx, 'Ash Stonefolk', /^Subrace/);
    await ctx.browser.locator('#hb-f-parent-input').fill('Stonefolk');
    await setNumber(ctx, 'charisma', 1);
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await save(ctx, 'Ash Stonefolk', PACK);
    await kindCount(ctx, /Subraces \(1\)/);
  });

  test('flow 5: a 2024 background with three abilities and an origin feat; a 2014 background with characteristics', async (ctx) => {
    await bootToHome(ctx, START);
    await newPack(ctx, PACK, 'Both editions');
    await newElement(ctx, 'Ember Warden', /^Background A background/);
    for (const a of ['Strength', 'Constitution', 'Wisdom']) {
      await ctx.screen.getByRole('button', a).tap();
      await expect(ctx.screen.getByRole('button', a)).toHaveAttribute('aria-pressed', 'true');
    }
    await ctx.screen.getByRole('button', 'Pick: Origin feat').tap();
    await expect(ctx.screen.getByRole('dialog', /^Choose: Origin feat/)).toBeVisible();
    await pickRow(ctx, 'Alert');
    await ctx.screen.getByRole('button', 'Pick: Skill proficiencies').tap();
    await expect(ctx.screen.getByRole('dialog', 'Choose proficiencies')).toBeVisible();
    await pickRow(ctx, 'Insight');
    await pickRow(ctx, 'Religion');
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await save(ctx, 'Ember Warden', PACK);

    // Try it: the test character takes the background with its ability distribution.
    await openTryIt(ctx);
    await trialLevel(ctx, 1);
    expectEqual('the draft in its slot', 'Ember Warden', await textOf(ctx, '.hb-try-draft-name'));
    await closeTryIt(ctx);

    // 2014: the characteristics are lists the character picks from.
    await newElement(ctx, 'Wanderer', /^Background A background/);
    await ctx.browser.locator('#hb-f-edition-input').selectOption('2014');
    await ctx.screen.getByRole('tab', 'CHARACTERISTICS').tap();
    for (const [list, entries] of [['Personality traits', ['I watch the sky.', 'I hum old songs.']], ['Ideals', ['Freedom.']], ['Bonds', ['My road.']], ['Flaws', ['I never stay.']]] as const) {
      for (const [i, text] of entries.entries()) {
        await ctx.browser.locator(`[role="group"][aria-label="${list}"] button.hb-pick-btn`).click();
        await ctx.browser.locator(`input[aria-label="${list} ${i + 1}"]`).fill(text);
      }
    }
    await waitValue(ctx, 'input[aria-label="Personality traits 2"]', 'I hum old songs.');
    await save(ctx, 'Wanderer', PACK);
    await ctx.screen.getByRole('tab', 'CHARACTERISTICS').tap();
    await waitValue(ctx, 'input[aria-label="Flaws 1"]', 'I never stay.');
  });
  test('flow 6: a +1 magic weapon made from a base weapon, tried equipped; armor with stealth disadvantage', async (ctx) => {
    await bootToHome(ctx, START);
    await newPack(ctx, PACK);
    await newElement(ctx, 'Ember Blade', /^Magic item/);
    await ctx.browser.locator('#hb-f-magicType-input').fill('Weapon');
    await ctx.browser.locator('#hb-f-rarity-input').fill('Uncommon');
    await reveal(ctx, 'select[aria-label="Made from a weapon: from"]');
    await chooseIn(ctx, 'select[aria-label="Made from a weapon: from"]', 'specific');
    await ctx.screen.getByRole('button', 'Pick: Made from a weapon: elements').tap();
    await expect(ctx.screen.getByRole('dialog', /^Choose/)).toBeVisible();
    await pickRow(ctx, 'Longsword');
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('dialog', /^Choose/)).toBeHidden();
    await reveal(ctx, '[data-field="enhancement"]');
    await ctx.screen.getByRole('button', 'Increase Bonus (+N)').tap();
    await waitValue(ctx, '[data-field="enhancement"] input[role="spinbutton"]', '1');
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await save(ctx, 'Ember Blade', PACK);
    // Try it: the test character carries it (the mock makes no attack of a magic item: the +1 to hit and damage on the
    // sheet is checked on the real backend, homebrew_flows.rs flow 6).
    await openTryIt(ctx);
    await trialLevel(ctx, 1);
    await expect(ctx.screen.getByText('Added to the inventory.')).toBeVisible();
    await closeTryIt(ctx);

    // Armor: heavy, AC 17, disadvantage on Stealth; a new armor is worn on the body, so a character can put it on.
    await newElement(ctx, 'Ash Plate', /^Armor Armor or a shield/);
    await waitValue(ctx, '#hb-f-slot-input', 'body');
    await chooseIn(ctx, '#hb-f-armorType-input', 'Heavy');
    await setNumber(ctx, 'baseAc', 17);
    await ctx.browser.locator('[data-field="stealth"] input[type="checkbox"]').check();
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await save(ctx, 'Ash Plate', PACK);
    await waitFor(ctx, (_: null) => (document.querySelector('[data-field="stealth"] input[type="checkbox"]') as HTMLInputElement | null)?.checked === true, null, 'stealth disadvantage kept');
    await openTryIt(ctx);
    await trialLevel(ctx, 1);
    await expect(ctx.screen.getByText('Added to the inventory and equipped.')).toBeVisible();
    await closeTryIt(ctx);
  });
  test('flow 7: copy the SRD Fireball, edit and save it; deep-copy an SRD class with its subclass', async (ctx) => {
    await bootToHome(ctx, START);
    await newPack(ctx, PACK);
    await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
    await ctx.screen.getByRole('radio', /^Spell A spell/).tap();
    await ctx.screen.getByRole('radio', /^Start from a template/).tap();
    await ctx.screen.getByRole('button', 'Choose...').tap();
    await expect(ctx.screen.getByRole('dialog', /Spell template/)).toBeVisible();
    await ctx.screen.getByRole('button', /Fireball/).first().tap();
    await ctx.screen.getByRole('button', 'Choose').tap();
    await waitValue(ctx, NAME, 'Fireball (copy)', LOAD_MS);
    await ctx.browser.locator(NAME).fill('Emberstorm');
    await ctx.browser.locator('#hb-f-level-input').selectOption('4th level');
    await expect(ctx.screen.getByRole('button', /Problems \(0\)/)).toBeVisible();
    await save(ctx, 'Emberstorm', PACK);
    await waitValue(ctx, '#hb-f-level-input', '4');

    await ctx.screen.getByRole('button', 'New').first().tap();
    await ctx.screen.getByRole('radio', /^Class A class/).tap();
    await ctx.screen.getByRole('radio', /^New from copy/).tap();
    await expect(ctx.screen.getByRole('checkbox', 'Copy its features and subclasses too')).toBeChecked();
    await ctx.screen.getByRole('button', 'Choose...').tap();
    await expect(ctx.screen.getByRole('dialog', /Class to copy/)).toBeVisible();
    await ctx.screen.getByRole('button', /Wizard.*SRD 5\.2\.1/).first().tap();
    await ctx.screen.getByRole('button', 'Choose').tap();
    await waitValue(ctx, NAME, 'Wizard (copy)', LOAD_MS);
    await save(ctx, 'Wizard (copy)', PACK);
    await kindCount(ctx, /Classes \(1\)/);
    await kindCount(ctx, /Subclasses \(1\)/);
  });

  test('flow 8: undo and redo across fields and blocks; leaving mid-edit keeps the draft and reopening resumes it', async (ctx) => {
    await bootToHome(ctx, START);
    await newPack(ctx, PACK);
    await newElement(ctx, 'Ember Lance');
    await ctx.browser.locator('#hb-f-school-input').selectOption('Evocation');
    await save(ctx, 'Ember Lance', PACK);

    // Three edits: a text field, a combo, a rule block.
    await ctx.browser.locator(NAME).fill('Ember Spear');
    await ctx.browser.locator('#hb-f-level-input').selectOption('3rd level');
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await addRule(ctx, 'Advantage on a roll');
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('listitem', /Advantage on a roll/)).toBeVisible();

    // Undo walks back the block (button), the level and the name (Ctrl+Z); redo brings each back in order.
    await clickCss(ctx, '.hb-commands button[aria-label="Undo"]');
    await expect(ctx.screen.getByRole('listitem', /Advantage on a roll/)).toBeHidden();
    await ctx.screen.getByRole('tab', 'GENERAL').tap();
    await pressKey(ctx, 'Control+z');
    await waitValue(ctx, '#hb-f-level-input', '1');
    await pressKey(ctx, 'Control+z');
    await waitValue(ctx, NAME, 'Ember Lance');
    await pressKey(ctx, 'Control+Shift+z');
    await waitValue(ctx, NAME, 'Ember Spear');
    await pressKey(ctx, 'Control+Shift+z');
    await waitValue(ctx, '#hb-f-level-input', '3');
    await clickCss(ctx, '.hb-commands button[aria-label="Redo"]');
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await expect(ctx.screen.getByRole('listitem', /Advantage on a roll/)).toBeVisible();
    await autosaved(ctx);

    // Leave without saving (nothing asks: the draft is kept, spec 10.1), come back, open the element: the draft resumes.
    // A real kill and relaunch runs on the real backend (homebrew_flows.rs flow 8) and on the devices (scripts/devices).
    await ctx.screen.getByRole('button', 'All packs').tap();
    await waitFor(ctx, (_: null) => document.querySelector('[aria-label="Homebrew packs"]') !== null, null, 'the pack list');
    await dblClickCss(ctx, '[aria-label="Homebrew packs"] [role=row][aria-selected]');
    await ctx.screen.getByRole('button', 'Ember Lance').first().tap();
    await ctx.screen.getByRole('button', 'Keep editing').tap();
    await waitValue(ctx, NAME, 'Ember Spear', LOAD_MS);
    await waitValue(ctx, '#hb-f-level-input', '3');
    await ctx.screen.getByRole('tab', 'RULES').tap();
    await expect(ctx.screen.getByRole('listitem', /Advantage on a roll/)).toBeVisible();
  });

  test('flow 9: export the pack, remove it, import the file and edit again', async (ctx) => {
    await bootToHome(ctx, START);
    await newPack(ctx, PACK);
    await newElement(ctx, 'Ember Lance');
    await ctx.browser.locator('#hb-f-school-input').selectOption('Evocation');
    await save(ctx, 'Ember Lance', PACK);

    await ctx.screen.getByRole('button', 'All packs').tap();
    await waitFor(ctx, (_: null) => document.querySelector('[aria-label="Homebrew packs"]') !== null, null, 'the pack list');
    await catchDownloads(ctx);
    await ctx.browser.locator('.hb-pack-grid .v4-grid-row').first().click();
    await ctx.screen.getByRole('button', 'Export pack').tap();
    await expect(ctx.screen.getByRole('dialog', 'Export pack')).toBeVisible();
    await ctx.browser.locator('[role="dialog"] button:text-is("Export")').click();
    const file = await takeDownload(ctx);
    expectEqual('the pack file', 'Ember_Isles.borealis-pack.zip', file.name);

    // Remove the pack where packs are removed (START > ADDITIONAL CONTENT), then import the file: a new pack again.
    await ctx.browser.evaluate((_: null) => { window.location.hash = '#/content'; return true; }, null);
    const row = `.v4-pack-row:has(strong:text-is("${PACK}"))`;
    await ctx.browser.locator(`${row} button:text-is("Remove")`).click();
    await expect(ctx.screen.getByRole('dialog', 'Remove pack')).toBeVisible();
    await ctx.screen.getByRole('button', 'Remove').last().tap();
    await waitFor(ctx, (name: string) => ![...document.querySelectorAll('.v4-pack-row strong')].some((n) => n.textContent === name), PACK, 'the pack removed');
    await ctx.browser.evaluate((_: null) => { window.location.hash = '#/homebrew'; return true; }, null);
    await waitFor(ctx, (_: null) => document.querySelector('[aria-label="Homebrew packs"]') !== null && document.querySelectorAll('.hb-pack-grid .v4-grid-row').length === 0, null, 'no packs');
    const saved = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'hc-flows-')), file.name);
    fs.writeFileSync(saved, Buffer.from(file.base64, 'base64'));
    await ctx.browser.locator('input[aria-label="Choose a pack file"]').setInputFiles(saved);
    await expect(ctx.screen.getByRole('dialog', 'Import preview')).toBeVisible();
    await ctx.screen.getByRole('button', 'Install').tap();
    await waitFor(ctx, (want: string) => [...document.querySelectorAll('h1, h2, h3')].some((h) => h.textContent === want), `Homebrew: ${PACK}`, 'the imported pack opens', LOAD_MS);
    await ctx.screen.getByRole('button', 'Ember Lance').tap();
    await waitValue(ctx, NAME, 'Ember Lance');
    await ctx.browser.locator(NAME).fill('Ember Lance II');
    await save(ctx, 'Ember Lance II', PACK);
    // Send/Receive between two devices is the real link: homebrew_flows.rs flow 9 (two AppStates over loopback).
  });
});
