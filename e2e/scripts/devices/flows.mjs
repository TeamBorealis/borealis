// The creator's device flows (spec 18.4 flow 10: flows 1, 3, 4 and 8 on a phone) and the phone budget (spec 18.5:
// save + reload < 4 s). Every step is a real touch through the phone UI: bottom sheets, U22 selects, the pinned
// Undo / Redo / Try it / Save row. Try it is the real engine on the device, so it stands for "a character takes it".

import { sleep } from './driver.mjs';
import { kill, launch } from './install.mjs';

export const PACK = 'Ember Isles';
const NAME = '#hb-f-name-input';
const STATUS = '.aur-status';
/** Spec 18.5: save + reload on a phone. */
export const PHONE_SAVE_BUDGET_MS = 4_000;
/** Saves measured per element for the budget. */
const BUDGET_SAVES = 3;

const re = (s) => `^${s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`;

async function check(d, expr, what) {
  if (!(await d.ev(`(() => { try { return !!(${expr}); } catch { return false; } })()`))) throw new Error(`check failed: ${what}`);
}

/** A U22 select: the select button opens the bottom sheet, a tap on the option chooses it and closes the sheet. */
async function sheetSelect(d, css, option) {
  await d.tap(css);
  await d.waitFor(`document.querySelector('.select-sheet-list')`, `the sheet of ${css}`);
  await d.tap('.select-sheet-list [role="option"]', re(option));
  await d.waitFor(`!document.querySelector('.select-sheet-list')`, 'the sheet closed');
}

/** The pinned row steps aside while the keyboard is up (kb-open): put the keyboard away first, as a user does. */
async function pinned(d, label) {
  if (await d.ev(`document.documentElement.classList.contains('kb-open')`)) {
    // the field gives up focus, as when the user taps outside it (agent-device's iOS "keyboard dismiss" is not reliable)
    await d.ev(`document.activeElement instanceof HTMLElement && document.activeElement.blur(); true`);
    await d.waitFor(`!document.documentElement.classList.contains('kb-open')`, 'the keyboard put away');
  }
  await d.tap(`.hb-commands.eq-pin button[aria-label="${label}"]`);
}
const autosaved = (d) => d.waitFor(`/All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? '')`, 'autosave');
const tab = (d, name) => d.tap('[role="tab"]', re(name));

/** Pinned Save, timed in the page from the tap's click event to the status line's "Saved '<name>'". */
export async function timedSave(d, name) {
  await autosaved(d);
  await d.ev(`(() => {
    const w = window; w.__save = { t0: 0, t1: 0 };
    const btn = document.querySelector('.hb-commands.eq-pin button[aria-label="Save"]');
    btn.addEventListener('click', () => { w.__save.t0 = performance.now(); }, { capture: true, once: true });
    const status = document.querySelector('${STATUS}');
    const mo = new MutationObserver(() => {
      if (w.__save.t0 && !w.__save.t1 && (status.textContent ?? '').includes(${JSON.stringify(`Saved '${name}'`)})) { w.__save.t1 = performance.now(); mo.disconnect(); }
    });
    mo.observe(status, { subtree: true, childList: true, characterData: true });
    return true;
  })()`);
  await pinned(d, 'Save');
  await d.waitFor('window.__save.t1 > 0', `"Saved '${name}'"`, 60_000);
  return Math.round(await d.ev('window.__save.t1 - window.__save.t0'));
}

async function openHomebrew(d) {
  await d.ev(`location.hash = '#/homebrew'; true`);
  await d.waitFor(`document.querySelector('[aria-label="Homebrew packs"]')`, 'the pack list');
}

async function newPack(d, name, edition) {
  await openHomebrew(d);
  await d.tap('button', '^New pack$');
  await d.fill('#hb-pack-name', name);
  if (edition) await sheetSelect(d, '#hb-pack-edition', edition);
  await d.tap('[role="dialog"] button', '^Create$');
  await d.waitFor(`[...document.querySelectorAll('h1, h2, h3')].some((h) => h.textContent === ${JSON.stringify(`Homebrew: ${name}`)})`, `the workspace of ${name}`);
}

async function newElement(d, card, name) {
  await d.tap('.hb-tools button, button', '^New( element)?$');
  await d.waitFor(`document.querySelector('#hb-new-name')`, 'New element');
  await d.fill('#hb-new-name', name);
  await d.tap('[role="radio"]', card);
  await d.tap('[role="dialog"] button', '^Create$');
  await d.waitValue(NAME, name);
  await sleep(600);
}

async function backToList(d) {
  await d.ev(`document.querySelector('.aur-content') && (document.querySelector('.aur-content').scrollTop = 0); true`);
  await d.tap('.hb-back');
  await sleep(500);
}

async function tryIt(d, level) {
  await pinned(d, 'Try it');
  await d.waitFor(`/level \\d+/.test(document.querySelector('.hb-try-status')?.textContent ?? '')`, 'the Try it test character', 60_000);
  if (level === undefined) return;
  const at = () => d.ev(`Number(/level (\\d+)/.exec(document.querySelector('.hb-try-status')?.textContent ?? '')?.[1])`);
  if ((await at()) !== level) await d.tap(`.hb-try-ladder tr[data-level="${level}"] button.ap-name`);
  await d.waitFor(`(document.querySelector('.hb-try-status')?.textContent ?? '').includes('level ${level}')`, `Try it at level ${level}`, 60_000);
}

const closeTryIt = async (d) => { await d.tap('[role="dialog"] button', '^Close$'); await d.waitFor(`!document.querySelector('.hb-try-status')`, 'Try it closed'); };

/** Flow 1: a 2024 spell on the Wizard list; the trial Wizard learns it at 3, where it casts 2nd-level spells. */
export async function flow1(d, out) {
  await newPack(d, PACK);
  await newElement(d, '^Spell\\s*A spell', 'Ember Lance');
  await sheetSelect(d, '#hb-f-level-input', '2nd level');
  await sheetSelect(d, '#hb-f-school-input', 'Evocation');
  await d.tap('[data-field="spellLists"] input[aria-label="Wizard"]');
  await d.waitFor(`document.querySelector('[data-field="spellLists"] input[aria-label="Wizard"]').checked`, 'Wizard list checked');
  d.shot('flow1-01-spell');
  out.saves.push({ what: 'spell (new)', ms: await timedSave(d, 'Ember Lance') });
  await tryIt(d, 3);
  await d.waitFor(`/Ember Lance/.test(document.querySelector('.hb-try [aria-label="Spells"]')?.textContent ?? '')`, 'Ember Lance among the trial Wizard spells at 3');
  await check(d, `/2nd/.test(document.querySelector('.hb-try [aria-label="Spell slots"]')?.textContent ?? '')`, '2nd-level slots at 3');
  d.shot('flow1-02-try-it-level-3');
  await closeTryIt(d);
}

/** Flow 3: a half-caster class; the LEVELS row sheet at 3; Try it's ladder clean to 20; then as a second class. */
export async function flow3(d, out) {
  await backToList(d);
  await newElement(d, '^Class\\s*A class', 'Gloom Knight');
  await sheetSelect(d, '#hb-f-hitDie-input', 'd10');
  // PROFICIENCIES: the skills a character chooses from (multi-select picker: a tap on a row's name checks it)
  await tab(d, 'PROFICIENCIES');
  await d.tap('[data-field="skills"] .hb-pick-btn');
  await d.waitFor(`document.querySelectorAll('.hb-pick-list .pick-row').length > 0`, 'the skill rows', 60_000);
  for (const skill of ['Athletics', 'Perception', 'Stealth']) await d.tap(`.hb-pick-list .pick-row[data-row="${skill}"] .pick-main`);
  await d.tap('[role="dialog"] button', '^OK$');
  await d.waitFor(`/Stealth/.test(document.querySelector('[data-field="skills"]')?.textContent ?? '')`, 'three skills chosen');
  await tab(d, 'LEVELS');
  await d.tap('.hb-level-rows li[data-level="3"] .hb-level-row');
  await d.waitFor(`[...document.querySelectorAll('[role="dialog"] h2')].some((h) => h.textContent === 'Level 3')`, 'the Level 3 sheet');
  await check(d, `/Subclass/.test(document.querySelector('[role="dialog"]:last-of-type')?.textContent ?? '')`, 'the subclass choice at 3');
  d.shot('flow3-01-level-3-sheet');
  await d.tap('[role="dialog"] button', '^Close$');
  await tab(d, 'SPELLCASTING');
  await d.tap('button', '^Add spellcasting$');
  await sheetSelect(d, '[data-field="casterType"] .ph-select, [data-field="casterType"] select', 'Half caster (round up)');
  await sheetSelect(d, '[data-field="ability"] .ph-select, [data-field="ability"] select', 'Wisdom');
  await d.fill('[data-field="spellList"] input', 'Ranger');
  await d.waitFor(`[...document.querySelectorAll('table[aria-label="Spell slots and spells by level"] tr[data-level="5"] .hb-slot-cell')].map((c) => c.textContent).join(',') === '4,2,-,-,-'`, 'half-caster slots at 5');
  await tab(d, 'EQUIPMENT');
  await d.tap('button', '^Add starting equipment$');
  await d.waitValue('#hb-package-name', 'Starting Equipment (Gloom Knight)');
  d.shot('flow3-02-class');
  out.saves.push({ what: 'class (new)', ms: await timedSave(d, 'Gloom Knight') });
  await tryIt(d);
  await d.waitFor(`document.querySelectorAll('.hb-try-ladder tr[data-level]').length === 20`, '20 ladder rows', 60_000);
  await check(d, `document.querySelectorAll('.hb-try-ladder .hb-try-problem').length === 0`, 'the ladder is clean');
  await d.tap('.hb-try-ladder tr[data-level="20"] button.ap-name');
  await d.waitFor(`(document.querySelector('.hb-try-status')?.textContent ?? '').includes('level 20')`, 'Try it at level 20', 60_000);
  d.shot('flow3-03-try-it-level-20');
  await d.tap('.hb-try-setup input[type="checkbox"]');
  await d.waitFor(`/starts as a Fighter/.test(document.querySelector('.hb-try-setup')?.textContent ?? '')`, 'tried as a second class after Fighter 1');
  await closeTryIt(d);
}

/** Flow 4: a species with a lineage choice and darkvision (the trial offers the lineage); a 2014 race and subrace. */
export async function flow4(d, out) {
  await newPack(d, 'Stone Coast', 'Both editions');
  await newElement(d, '^Species', 'Emberkin');
  await d.fill('[data-field="darkvision"] input[role="spinbutton"]', '60');
  await d.tap('button', '^Add a racial trait$');
  await d.waitFor(`document.querySelector('#hb-f-c-n1-name-input')`, 'the trait sheet');
  await d.fill('#hb-f-c-n1-name-input', 'Ember Lineage');
  for (const [key, name] of [['n2', 'Ash'], ['n3', 'Cinder']]) {
    await d.tap('button', '^Add a species trait option$');
    await d.waitFor(`document.querySelector('#hb-f-c-${key}-name-input')`, `the option sheet ${key}`);
    await d.fill(`#hb-f-c-${key}-name-input`, name);
    await d.tap(`[role="dialog"]:has([data-child="${key}"]) button`, '^Done$');
    await d.waitFor(`!document.querySelector('[data-child="${key}"]')`, `the ${name} sheet closed`);
  }
  await d.tap('[role="dialog"]:has([data-child="n1"]) button', '^Done$');
  await d.waitFor(`!document.querySelector('[data-child="n1"]')`, 'the trait sheet closed');
  d.shot('flow4-01-species');
  out.saves.push({ what: 'species (new)', ms: await timedSave(d, 'Emberkin') });
  await tryIt(d);
  await d.waitFor(`/Cinder/.test(document.querySelector('.hb-try')?.textContent ?? '')`, 'the lineage choice offers its options');
  d.shot('flow4-02-try-it-lineage');
  await closeTryIt(d);
  await backToList(d);
  await newElement(d, '^Race\\s*A 2014', 'Stonefolk');
  await d.fill('[data-field="constitution"] input[role="spinbutton"]', '2');
  out.saves.push({ what: 'race (new)', ms: await timedSave(d, 'Stonefolk') });
  await backToList(d);
  await newElement(d, '^Subrace', 'Ash Stonefolk');
  await d.fill('#hb-f-parent-input', 'Stonefolk');
  await d.fill('[data-field="charisma"] input[role="spinbutton"]', '1');
  out.saves.push({ what: 'subrace (new)', ms: await timedSave(d, 'Ash Stonefolk') });
  d.shot('flow4-03-subrace');
}

/** Flow 8: undo / redo across a field and a block (pinned row), then the app killed mid-edit; the draft comes back. */
export async function flow8(d, out) {
  await openHomebrew(d);
  await d.tap('.hb-pack-grid .v4-grid-row', PACK);
  await d.tap('button', '^Open pack$');
  await d.tap('button', '^Ember Lance$');
  // A draft of the saved spell can be left over from its save (known item PVTI_lAHODah7es4Bp0kdzhH5wg4): start from the
  // saved spell.
  await sleep(1500);
  if (await d.ev(`[...document.querySelectorAll('[role="dialog"] button')].some((b) => b.textContent === 'Discard')`)) {
    out.leftoverDraft = true;
    await d.tap('[role="dialog"] button', '^Discard$');
  }
  await d.waitValue(NAME, 'Ember Lance');
  await d.fill(NAME, 'Ember Spear');
  await tab(d, 'RULES');
  await d.tap('button', '^Add rule$');
  await d.tap('button', '^Advantage on a roll$');
  await sheetSelect(d, '[data-field="roll"] .ph-select', 'Saving throws');
  await d.tap('[role="dialog"] button', '^OK$');
  await d.waitFor(`/Advantage on a roll/.test(document.querySelector('.hb-block-list')?.textContent ?? '')`, 'the rule added');
  await pinned(d, 'Undo');
  await d.waitFor(`!/Advantage on a roll/.test(document.querySelector('.hb-block-list')?.textContent ?? '')`, 'the rule undone');
  await pinned(d, 'Undo');
  await tab(d, 'GENERAL');
  await d.waitValue(NAME, 'Ember Lance');
  await pinned(d, 'Redo');
  await d.waitValue(NAME, 'Ember Spear');
  await pinned(d, 'Redo');
  await tab(d, 'RULES');
  await d.waitFor(`/Advantage on a roll/.test(document.querySelector('.hb-block-list')?.textContent ?? '')`, 'the rule redone');
  await autosaved(d);
  d.shot('flow8-01-before-kill');
  kill(d);
  await sleep(1500);
  await launch(d);
  await openHomebrew(d);
  await d.tap('.hb-pack-grid .v4-grid-row', PACK);
  await d.tap('button', '^Open pack$');
  await d.tap('button', '^Ember Lance$');
  await d.tap('[role="dialog"] button', '^Keep editing$');
  await tab(d, 'GENERAL');
  await d.waitValue(NAME, 'Ember Spear');
  await tab(d, 'RULES');
  await d.waitFor(`/Advantage on a roll/.test(document.querySelector('.hb-block-list')?.textContent ?? '')`, 'the rule restored');
  d.shot('flow8-02-draft-restored');
}

/** Phone budget: save + reload of a spell and of a deep copy of an SRD class (its features and subclass inside). */
export async function budget(d, out) {
  await backToList(d);
  await d.tap('.hb-tools button, button', '^New( element)?$');
  await d.tap('[role="radio"]', '^Class\\s*A class');
  await d.tap('[role="radio"]', '^New from copy');
  await d.tap('button', '^Choose\\.\\.\\.$');
  await d.waitFor(`document.querySelector('.hb-picker .pick-fsearch, [role="dialog"] input[type="search"]')`, 'the class picker', 60_000);
  await d.fill('.hb-picker .pick-fsearch, [role="dialog"] input[type="search"]', 'wizard');
  await d.tap('[role="dialog"] button', 'Wizard.*SRD 5\\.2\\.1');
  await d.tap('[role="dialog"] button', '^Choose$');
  await d.waitValue(NAME, 'Wizard (copy)');
  for (let i = 1; i <= BUDGET_SAVES; i += 1) {
    const name = `Ember Wizard ${i}`;
    await d.fill(NAME, name);
    out.saves.push({ what: 'deep-copied 2024 Wizard class', ms: await timedSave(d, name) });
  }
  await backToList(d);
  await d.tap('button', '^Spells \\(\\d+\\)$');
  await d.tap('button', '^Ember (Lance|Spear)$');
  await sleep(1500);
  if (await d.ev(`[...document.querySelectorAll('[role="dialog"] button')].some((b) => b.textContent === 'Keep editing')`)) await d.tap('[role="dialog"] button', '^Keep editing$');
  await tab(d, 'GENERAL');
  for (let i = 1; i <= BUDGET_SAVES; i += 1) {
    const name = `Ember Lance ${i}`;
    await d.fill(NAME, name);
    out.saves.push({ what: 'spell', ms: await timedSave(d, name) });
  }
}
