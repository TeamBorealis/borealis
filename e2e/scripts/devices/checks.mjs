// Device checks of the creator's phone UI that the web mock can't settle (lane F of HC13): SPELL LISTS' Select mode
// (H0eao), the cell sheet's Bold and Italic with the real keyboard (Idecg), what the platform accessibility tree says
// about the New element kind cards and the check boxes (H-a5M, H-a_A), and on Android the share sheet and a shared file
// Borealis does not open (H5wn8). Each check makes its own pack, so any of them runs alone (`--only`).

import { execFileSync } from 'node:child_process';
import { BUNDLE, sleep } from './driver.mjs';
import { check, newElement, newPack, openHomebrew, re, sheetSelect, tab } from './flows.mjs';

const LISTS = '.hb-lists';
/** Pack names of this run: a check can run again on a kept install (`--no-install`) without meeting its last pack. */
const RUN = Date.now().toString(36).slice(-4);
const packName = (base) => `${base} ${RUN}`;
const CELL_BOX = '.rich-cell-sheet .rich-cell-edit';
/** The soft keyboard takes this long to come up after a text box takes the focus. */
const KEYBOARD_MS = 1_500;
/** A toast lasts a few seconds: it is looked for this long. */
const TOAST_MS = 8_000;
/** The Android share sheet (chooser) takes this long to come up. */
const CHOOSER_MS = 4_000;

const selectedSpells = (d) => d.ev(`[...document.querySelectorAll('${LISTS} table tr[aria-selected="true"] button.ap-name')].map((b) => b.textContent).sort().join(',')`);

/** H0eao: SPELL LISTS Select mode chooses rows with taps, Add selected checks them, leaving the mode clears the choice. */
export async function spellLists(d, out) {
  await newPack(d, packName('List Isles'));
  await d.tap('button', '^Spell Lists$');
  await d.waitFor(`document.querySelector('[aria-label="Spell lists"]')`, 'the SPELL LISTS view');
  await sheetSelect(d, `${LISTS} .ph-select[aria-label="Spell list"]`, 'Wizard');
  await d.waitFor(`document.querySelectorAll('${LISTS} table button.ap-name').length > 0`, 'the spell grid', 60_000);
  await d.tap(`${LISTS} [aria-label="Spell list commands"] button`, '^Select$');
  await check(d, `document.querySelector('${LISTS} [aria-label="Spell list commands"] button[aria-pressed]')?.getAttribute('aria-pressed') === 'true'`, 'Select mode on');
  for (const name of ['Bless', 'Guidance', 'Cure Wounds', 'Cure Wounds']) await d.tap(`${LISTS} table button.ap-name`, re(name));
  const chosen = await selectedSpells(d);
  if (chosen !== 'Bless,Guidance') throw new Error(`the tapped rows: ${chosen} (want Bless,Guidance; a second tap takes one out)`);
  await check(d, `/^2 spells chosen$/.test(document.querySelector('${LISTS} .hb-lists-chosen')?.textContent ?? '')`, '"2 spells chosen"');
  d.shot('lists-01-two-chosen');
  await d.tap(`${LISTS} [aria-label="Spell list commands"] button`, '^Add selected$');
  await d.waitFor(`/\\(2 not saved\\)/.test(document.querySelector('${LISTS} .hb-lists-count')?.textContent ?? '')`, 'two checks waiting for Save');
  // the grid can hold a spell of each edition under one name: the chosen row's box is checked
  await check(d, `['Bless', 'Guidance'].every((n) => [...document.querySelectorAll('${LISTS} input[aria-label="Choose ' + n + '"]')].some((i) => i.checked))`, 'both spells checked');
  d.shot('lists-02-added');
  await d.tap(`${LISTS} [aria-label="Spell list commands"] button`, '^Select$');
  const left = await selectedSpells(d);
  if (left !== '') throw new Error(`leaving Select mode left ${left} chosen`);
  out.checks.push({ name: 'spellLists', ok: true, detail: 'taps chose Bless, Guidance (Cure Wounds tapped twice: out); Add selected checked both; leaving the mode cleared the choice' });
}

/** The page's view of the keyboard and the cell sheet: the sheet and its box must stay above the keyboard, unpanned. */
const keyboardState = (d) => d.ev(`(() => {
  const vv = window.visualViewport;
  const sheet = document.querySelector('.rich-cell-sheet')?.getBoundingClientRect();
  const box = document.querySelector('${CELL_BOX}')?.getBoundingClientRect();
  const bottom = vv.offsetTop + vv.height;
  return { kbOpen: document.documentElement.classList.contains('kb-open'), kb: getComputedStyle(document.documentElement).getPropertyValue('--kb').trim(),
    viewport: { offsetTop: Math.round(vv.offsetTop), height: Math.round(vv.height) }, innerHeight, scrollY: Math.round(scrollY),
    sheetBottom: Math.round(sheet?.bottom ?? -1), boxTop: Math.round(box?.top ?? -1), boxBottom: Math.round(box?.bottom ?? -1),
    boxVisible: !!box && box.top >= vv.offsetTop && box.bottom <= bottom + 1, focused: document.activeElement?.matches('${CELL_BOX}') ?? false };
})()`);

/** Selects the cell's whole text (a finger's long press and handles; the page sets the same DOM selection). */
async function selectCellText(d) {
  await d.ev(`(() => { const el = document.querySelector('${CELL_BOX}'); const r = document.createRange(); r.selectNodeContents(el);
    getSelection().removeAllRanges(); getSelection().addRange(r); return true; })()`);
  await d.waitFor(`getSelection().toString() === document.querySelector('${CELL_BOX}').textContent`, 'the cell text selected');
}

const cellHtml = (d) => d.ev(`document.querySelector('${CELL_BOX}')?.innerHTML ?? null`);
const heads = (d) => d.ev(`[...document.querySelectorAll('.rich-host .rich-edit th')].map((th) => th.innerHTML).join(' | ')`);

/** Idecg: the phone cell sheet with the real keyboard; Bold and Italic taps keep the selection; the tree names them. */
export async function cellMarks(d, out) {
  await newPack(d, packName('Cell Isles'));
  await newElement(d, '^Spell\\s*A spell', 'Cell Lance');
  await tab(d, 'DESCRIPTION');
  await d.tap('.rich-edit');
  await d.tap('button[aria-label="Insert table"]');
  await d.waitFor(`document.querySelectorAll('.rich-host .rich-edit th').length === 2`, 'a new table');
  await d.ev(`document.activeElement instanceof HTMLElement && document.activeElement.blur(); true`);
  await d.tap('button[aria-label="Edit cell"]');
  await d.waitFor(`[...document.querySelectorAll('[role="dialog"] h2')].some((h) => h.textContent === 'Table cell: Header, column 1')`, 'the cell sheet');
  // the real keyboard: a tap in the box
  await d.tap(CELL_BOX);
  await sleep(KEYBOARD_MS);
  const keyboard = await keyboardState(d);
  d.shot('cell-01-keyboard-up');
  if (!keyboard.focused) throw new Error(`the cell box did not take the focus: ${JSON.stringify(keyboard)}`);
  if (!keyboard.boxVisible || keyboard.scrollY !== 0) throw new Error(`the cell box is not above the keyboard, unpanned: ${JSON.stringify(keyboard)}`);
  await selectCellText(d);
  await d.tap('.rich-cell-tools button[aria-label="Bold"]');
  await d.waitFor(`document.querySelector('${CELL_BOX}').innerHTML === '<b>Column 1</b>'`, 'Column 1 bold');
  const keptAfterBold = await d.ev(`getSelection().toString()`);
  if (keptAfterBold !== 'Column 1') throw new Error(`Bold lost the selection (now "${keptAfterBold}")`);
  await d.tap('.rich-cell-tools button[aria-label="Italic"]');
  await d.waitFor(`document.querySelector('${CELL_BOX}').innerHTML === '<b><i>Column 1</i></b>'`, 'Column 1 bold and italic');
  await check(d, `['Bold', 'Italic'].every((b) => document.querySelector('.rich-cell-tools button[aria-label="' + b + '"]').getAttribute('aria-pressed') === 'true')`, 'Bold and Italic pressed');
  const afterMarks = { selection: await d.ev(`getSelection().toString()`), html: await cellHtml(d), ...(await keyboardState(d)) };
  d.shot('cell-02-bold-italic');
  const tree = d.axTree('cell-ax');
  const named = (label) => tree.filter((n) => n.label === label).map((n) => `${n.type}${n.value ? `=${n.value}` : ''}${n.checkable ? ' checkable' : ''}${n.checked ? ' checked' : ''}`);
  // the box is a text entry named "Cell text" (Android: an EditText whose hint carries the name, as TalkBack reads it)
  const entry = d.platform === 'android' ? /EditText$/ : /^(TextView|TextField)$/;
  const cellText = tree.filter((n) => entry.test(n.type) && (n.label === 'Cell text' || n.hint.startsWith('Cell text'))).map((n) => `${n.type} "${n.label}" ${n.hint}`.trim());
  const ax = { cellText, bold: named('Bold'), italic: named('Italic') };
  if (ax.cellText.length === 0 || ax.bold.length === 0 || ax.italic.length === 0) throw new Error(`the tree misses Cell text / Bold / Italic: ${JSON.stringify(ax)}`);
  await d.ev(`document.activeElement instanceof HTMLElement && document.activeElement.blur(); true`);
  await d.tap('[role="dialog"] button', '^OK$');
  await d.waitFor(`!document.querySelector('.rich-cell-sheet')`, 'the cell sheet closed');
  const written = await heads(d);
  if (written !== '<b><i>Column 1</i></b> | Column 2') throw new Error(`the description's headings: ${written}`);
  d.shot('cell-03-written');
  out.checks.push({ name: 'cellMarks', ok: true, keyboard, afterMarks, ax, written });
}

/** H-a5M / H-a_A: what the platform tree says about the kind cards (radios) and the creator's check boxes. */
export async function a11y(d, out) {
  await newPack(d, packName('Tree Isles'));
  await d.tap('.hb-tools button, button', '^New( element)?$');
  await d.waitFor(`document.querySelector('#hb-new-name')`, 'New element');
  await d.ev(`document.activeElement instanceof HTMLElement && document.activeElement.blur(); true`);
  await sleep(KEYBOARD_MS);
  // a card's node is named by its name and its one-line description
  const cards = await d.ev(`[...document.querySelectorAll('.hb-kind-card')].map((c) => [c.querySelector('.hb-kind-name')?.textContent ?? '', (c.querySelector('.hb-kind-desc')?.textContent ?? '').slice(0, 12)])`);
  const kinds = d.axTree('a11y-kinds').filter((n) => cards.some(([name, desc]) => n.label.startsWith(name) && n.label.includes(desc)));
  d.shot('a11y-01-kinds');
  if (kinds.length === 0) throw new Error('the kind cards are not in the tree');
  // the cards are radios: Android names the class RadioButton; WebKit gives a radio the radio trait, which XCTest shows
  // as "Other" with the checked state as its value (as for <input type="radio">), so the state must be there
  const stated = d.platform === 'android' ? kinds.every((n) => n.type === 'android.widget.RadioButton') : kinds.some((n) => n.value === '1') && kinds.some((n) => n.value === '0');
  if (!stated) throw new Error(`the kind cards are not radios with a state: ${JSON.stringify(kinds)}`);
  await d.fill('#hb-new-name', 'Tree Lance');
  await d.tap('[role="radio"]', '^Spell\\s*A spell');
  await d.tap('[role="dialog"] button', '^Create$');
  await d.waitValue('#hb-f-name-input', 'Tree Lance');
  await d.tap('#hb-f-concentration-input');
  await d.waitFor(`document.querySelector('#hb-f-concentration-input').checked`, 'Concentration checked');
  await d.ev(`document.querySelector('#hb-f-ritual-input')?.scrollIntoView({ block: 'center' }); true`);
  await sleep(600);
  const boxes = d.axTree('a11y-checks').filter((n) => n.label === 'Concentration' || n.label === 'Ritual');
  d.shot('a11y-02-checks');
  if (boxes.length < 2) throw new Error(`the check boxes are not in the tree: ${JSON.stringify(boxes)}`);
  out.checks.push({ name: 'a11y', ok: true, kinds: kinds.slice(0, 4), boxes });
}

/** H5wn8 (Android): Export pack > Share leaves Borealis out of the share sheet; a shared file Borealis won't open says why. */
export async function share(d, out) {
  if (d.platform !== 'android') { out.checks.push({ name: 'share', ok: true, detail: 'Android only' }); return; }
  const pack = packName('Share Isles');
  await newPack(d, pack);
  await openHomebrew(d);
  await d.tap('.hb-pack-grid [role="row"] .nm', `^${pack}`);
  await d.tap('button[aria-label="Export pack"]');
  await d.tap('[role="dialog"] button', '^Share$');
  await sleep(CHOOSER_MS);
  const chooser = d.axTree('share-chooser');
  d.shot('share-01-chooser');
  const labels = chooser.map((n) => n.label).filter(Boolean);
  if (!labels.some((l) => /share|Copy|Nearby|Drive|Gmail|Messages|Files|Bluetooth/i.test(l))) throw new Error(`no share sheet: ${labels.join(' | ')}`);
  if (labels.includes('Borealis')) throw new Error(`Borealis is offered in its own share sheet: ${labels.join(' | ')}`);
  d.adb('shell', 'input', 'keyevent', 'KEYCODE_BACK');
  await sleep(1_000);
  const toast = async (want, what) => {
    const end = Date.now() + TOAST_MS;
    for (;;) {
      const text = await d.ev(`[...document.querySelectorAll('.toasts .toast')].map((t) => t.textContent).join(' | ')`).catch(() => '');
      if (new RegExp(want).test(text)) return text;
      if (Date.now() > end) throw new Error(`no message for ${what} (toasts: ${text})`);
      await sleep(250);
    }
  };
  // Borealis's own file handed back to it (what choosing Borealis in the sheet did before): a message, not silence
  const own = d.adb('shell', 'run-as', BUNDLE, 'ls', '-t', 'cache/share').toString().split(/\s+/).find(Boolean);
  if (!own) throw new Error('the exported pack is not in cache/share');
  d.adb('shell', 'am', 'start', '-a', 'android.intent.action.SEND', '-t', 'application/zip', '--eu', 'android.intent.extra.STREAM', `content://${BUNDLE}.fileprovider/share/${own}`, '-n', `${BUNDLE}/.MainActivity`);
  const ownMessage = await toast('is a file Borealis shared itself', 'its own file');
  d.shot('share-02-own-file-message');
  // a file: URI (not a content URI another app shares)
  execFileSync(d.adbPath, ['-s', d.serial, 'shell', 'sh', '-c', "'echo \"<elements/>\" > /sdcard/Download/borealis-refused.xml'"]);
  d.adb('shell', 'am', 'start', '-a', 'android.intent.action.VIEW', '-d', 'file:///sdcard/Download/borealis-refused.xml', '-t', 'text/xml', '-n', `${BUNDLE}/.MainActivity`);
  const fileMessage = await toast('was not opened: Borealis opens only files another app shares', 'a file: URI');
  d.shot('share-03-file-uri-message');
  d.adb('shell', 'rm', '-f', '/sdcard/Download/borealis-refused.xml');
  out.checks.push({ name: 'share', ok: true, chooser: labels, ownMessage, fileMessage });
}
