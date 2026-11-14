// Device driver for the creator's device suite (spec 18.4 flow 10; docs/creator-device-e2e.md).
//
// The page is read and written through the WebView's inspector: Android's debug WebView DevTools socket (CDP), the iOS
// simulator's WebKit remote inspector (rwi.py). Every tap a user makes is a real touch on the device: `adb shell input
// tap` on Android, agent-device's XCTest press on iOS, at the element's centre (CSS px x devicePixelRatio on Android,
// points on iOS). Text goes in through the page (the React value setter) unless a step needs the real keyboard.

import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
export const APP_ROOT = join(HERE, '../../..');
export const BUNDLE = 'app.borealis';
const DEVELOPER_DIR = process.env.DEVELOPER_DIR ?? '/Applications/Xcode.app/Contents/Developer';
const ANDROID_HOME = process.env.ANDROID_HOME ?? join(homedir(), 'Library/Android/sdk');
const ADB = join(ANDROID_HOME, 'platform-tools/adb');
const AGENT_DEVICE = join(HERE, '../../node_modules/.bin/agent-device');
const CDP_PORT = Number(process.env.E2E_CDP_PORT ?? 9333);
const RWI_PORT = Number(process.env.E2E_RWI_PORT ?? 9334);
const AD_SESSION = 'borealis-creator-devices';
/** Waits on the page poll this often. */
const POLL_MS = 250;
/** Default budget of a wait (a Rust-backed screen on a debug build can take seconds). */
export const WAIT_MS = 30_000;
/** Let a tap's animation (a sheet rising, a tab switching) run before reading the page again. */
const AFTER_TAP_MS = 600;
/** A target must read the same centre twice this far apart, and be hit there, before it is touched (at most STILL_READS_MAX reads). */
const STILL_POLL_MS = 200;
const STILL_READS_MAX = 25;
/** How long a touch has to reach its target, and how often it is made. */
const CLICK_WAIT_MS = 3_000;
const TAP_TRIES = 6;
/** The phone status line folds this long after a plain message (ui/src/components/shell/Chrome.tsx STATUS_FOLD_MS). */
const STATUS_FOLD_MS = 4_000;
const STATUS_WAIT_MS = 3 * STATUS_FOLD_MS;
/** The soft keyboard takes this long to go away after its field gives up the focus. */
const KEYBOARD_AWAY_MS = 800;
/** iOS: tries of one XCTest press, and the pause between them. */
/** How long a timed-out iOS press may still take to land. */
const LATE_PRESS_WAIT_MS = 10_000;

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const env = { ...process.env, DEVELOPER_DIR };
const run = (cmd, args, opts = {}) => execFileSync(cmd, args, { env, maxBuffer: 256 * 1024 * 1024, stdio: ["ignore", "pipe", "pipe"], ...opts });

/** Page-side helpers, installed once per page load: query by CSS + text, fill through React's setter, rectangles. */
const HELPERS = `(() => {
  if (window.__dv) return true;
  const visible = (e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0; };
  const describe = (e) => !(e instanceof Element) ? String(e) : e.tagName.toLowerCase() + (e.id ? '#' + e.id : '')
    + (typeof e.className === 'string' && e.className ? '.' + e.className.trim().split(/\\s+/).slice(0, 2).join('.') : '');
  // every touch the page receives, so a touch that never reached its target can be told apart (IDpjU)
  const seen = [];
  for (const type of ['touchstart', 'touchend', 'pointerdown', 'pointerup', 'click']) {
    document.addEventListener(type, (ev) => {
      const p = ev.touches?.[0] ?? ev.changedTouches?.[0] ?? ev;
      seen.push({ type, x: Math.round(p.clientX ?? -1), y: Math.round(p.clientY ?? -1), target: describe(ev.target), t: Math.round(performance.now()) });
      if (seen.length > 40) seen.shift();
    }, { capture: true, passive: true });
  }
  window.__dv = {
    seen,
    /**
     * False while the phone status line is open and will fold by itself (a plain message folds 4 s after it shows,
     * STATUS_FOLD_MS in Chrome.tsx, moving everything pinned above it; a running command folds 4 s after it ends). A
     * folded line, or one held open by a pending choice or an offer, will not move on its own.
     */
    statusSettled() {
      const st = document.querySelector('.aur-status');
      if (!st || st.dataset.collapsed !== 'false') return true;
      if (st.querySelector('.aur-status-busyline')) return false;
      return st.dataset.pending === 'true' || st.querySelector('button') !== null;
    },
    /** What the page looked like around a touch that did not reach its target. */
    probe(x, y, since, sel, text) {
      const vv = window.visualViewport;
      const re = text === undefined || text === null ? null : new RegExp(text);
      const matches = [...document.querySelectorAll(sel)].filter((e) => !re || re.test((e.getAttribute('aria-label') ?? '').trim()) || re.test((e.textContent ?? '').trim()))
        .map((e) => { const r = e.getBoundingClientRect(); const cs = getComputedStyle(e);
          return { y: Math.round(r.y), h: Math.round(r.height), visibility: cs.visibility, hitSelf: e.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2)), path: [...(function* up(n) { while (n && n !== document.body) { yield describe(n); n = n.parentElement; } })(e)].slice(1, 6).join(' < ') }; });
      return {
        matches,
        viewport: vv && { offsetTop: Math.round(vv.offsetTop), offsetLeft: Math.round(vv.offsetLeft), pageTop: Math.round(vv.pageTop), scale: vv.scale, height: Math.round(vv.height), width: Math.round(vv.width) },
        scrollY: Math.round(window.scrollY), innerHeight: window.innerHeight,
        kbOpen: document.documentElement.classList.contains('kb-open'),
        active: describe(document.activeElement), hit: describe(document.elementFromPoint(x, y)),
        events: seen.filter((s) => s.t >= since),
      };
    },
    now() { return Math.round(performance.now()); },
    q(sel, text) {
      const all = [...document.querySelectorAll(sel)].filter(visible);
      if (text === undefined || text === null) return all[all.length - 1] ?? null;
      const re = new RegExp(text);
      return all.filter((e) => re.test((e.getAttribute('aria-label') ?? '').trim()) || re.test((e.textContent ?? '').trim())).pop() ?? null;
    },
    /** The target's centre and whether the page's hit test there lands on it (or inside it). */
    aim(sel, text) {
      const e = this.q(sel, text); if (!e) return null;
      const r = e.getBoundingClientRect(); const x = r.x + r.width / 2; const y = r.y + r.height / 2;
      const h = document.elementFromPoint(x, y);
      // the soft keyboard is up and the target is not the focused field's own control
      const kb = document.documentElement.classList.contains('kb-open') && !e.contains(document.activeElement) && !document.activeElement?.contains(e);
      return { x, y, hit: !!h && (h === e || e.contains(h)), kb };
    },
    reveal(sel, text) {
      const e = this.q(sel, text); if (!e) return false;
      // the pinned row is always on screen; scrolling "to" it scrolls the form to its end, and on iOS the touch right
      // after a scroll only stops the scroll
      if (e.closest('.eq-pin')) return true;
      e.scrollIntoView({ block: 'center', behavior: 'instant' });
      return true;
    },
    fill(sel, v) {
      const e = document.querySelector(sel); if (!e) throw new Error('no ' + sel);
      const proto = e.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : e.tagName === 'SELECT' ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
      Object.getOwnPropertyDescriptor(proto, 'value').set.call(e, v);
      e.dispatchEvent(new Event('input', { bubbles: true })); e.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    },
    text(sel) { return document.querySelector(sel)?.textContent ?? null; },
    value(sel) { return document.querySelector(sel)?.value ?? null; },
  };
  return true;
})()`;

const arg = (v) => (v === undefined ? 'undefined' : JSON.stringify(v));

// ---------------------------------------------------------------------------------------------------------- Android

async function adbSerial() {
  if (process.env.ANDROID_SERIAL) return process.env.ANDROID_SERIAL;
  const avd = process.env.E2E_ANDROID_DEVICE ?? 'borealis';
  const lines = run(ADB, ['devices']).toString().split('\n').slice(1).map((l) => l.split('\t')).filter((p) => p[1] === 'device');
  for (const [serial] of lines) {
    const name = run(ADB, ['-s', serial, 'emu', 'avd', 'name']).toString().split(/\r?\n/)[0]?.trim();
    if (name === avd) return serial;
  }
  throw new Error(`no running emulator for the AVD "${avd}"`);
}

/** CDP on the app's debug WebView: forward its DevTools socket and open the page target. */
async function cdpConnect(adb) {
  const pid = adb('shell', 'pidof', BUNDLE).toString().trim();
  if (!pid) throw new Error(`${BUNDLE} is not running`);
  adb('forward', `tcp:${CDP_PORT}`, `localabstract:webview_devtools_remote_${pid}`);
  let list = [];
  for (let i = 0; i < 40 && list.length === 0; i += 1) {
    try { list = (await (await fetch(`http://127.0.0.1:${CDP_PORT}/json`)).json()).filter((t) => t.type === 'page'); } catch { /* not up yet */ }
    if (list.length === 0) await sleep(POLL_MS * 2);
  }
  if (list.length === 0) throw new Error('no WebView page on the DevTools socket');
  const ws = new WebSocket(list[0].webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  let id = 0;
  const waiting = new Map();
  ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && waiting.has(d.id)) { waiting.get(d.id)(d); waiting.delete(d.id); } };
  const send = (method, params = {}) => new Promise((res) => { id += 1; waiting.set(id, res); ws.send(JSON.stringify({ id, method, params })); });
  return {
    async eval(expr) {
      const r = await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true });
      if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description ?? JSON.stringify(r.result.exceptionDetails));
      return r.result?.result?.value;
    },
    close() { ws.close(); },
  };
}

async function android(evidence) {
  const serial = await adbSerial();
  const adb = (...a) => run(ADB, ['-s', serial, ...a]);
  let cdp = null;
  let viewTop = null;
  return {
    platform: 'android',
    serial,
    adb,
    adbPath: ADB,
    async connect() { cdp?.close(); cdp = await cdpConnect(adb); viewTop = null; },
    async eval(expr) { return cdp.eval(expr); },
    async touch(x, y) {
      if (viewTop === null) {
        const xml = adb('exec-out', 'uiautomator', 'dump', '/dev/tty').toString();
        const m = /class="android.webkit.WebView"[^>]*bounds="\[(\d+),(\d+)\]/.exec(xml);
        viewTop = m ? Number(m[2]) : 0;
      }
      const dpr = await cdp.eval('devicePixelRatio');
      adb('shell', 'input', 'tap', String(Math.round(x * dpr)), String(Math.round(y * dpr + viewTop)));
      return true;
    },
    /** Real keyboard input into the focused field (adb input text: spaces as %s). */
    async type(text) { adb('shell', 'input', 'text', text.replace(/ /g, '%s')); },
    async hideKeyboard() { adb('shell', 'input', 'keyevent', 'KEYCODE_BACK'); },
    shot(name) { writeFileSync(join(evidence, `${name}.png`), adb('exec-out', 'screencap', '-p')); },
    /** The platform accessibility tree (what TalkBack's tree holds), as uiautomator reads it; the raw dump is kept. */
    axTree(name) {
      const xml = adb('exec-out', 'uiautomator', 'dump', '/dev/tty').toString();
      writeFileSync(join(evidence, `${name}.xml`), xml);
      const attr = (n, a) => new RegExp(`\\s${a}="([^"]*)"`).exec(n)?.[1] ?? '';
      return [...xml.matchAll(/<node [^>]*>/g)].map(([n]) => ({
        type: attr(n, 'class'), label: attr(n, 'text') || attr(n, 'content-desc'), hint: attr(n, 'hint'), value: '',
        checkable: attr(n, 'checkable') === 'true', checked: attr(n, 'checked') === 'true',
      }));
    },
    close() { cdp?.close(); },
  };
}

// ---------------------------------------------------------------------------------------------------------------- iOS

function simulatorUdid() {
  if (process.env.E2E_IOS_UDID) return process.env.E2E_IOS_UDID;
  const name = process.env.E2E_IOS_DEVICE ?? 'iPhone 18 Pro';
  const groups = JSON.parse(run('xcrun', ['simctl', 'list', 'devices', 'available', '-j']).toString()).devices;
  for (const list of Object.values(groups)) {
    const hit = list.find((d) => d.name === name);
    if (hit) return hit.udid;
  }
  throw new Error(`no simulator named ${name}`);
}

async function ios(evidence) {
  const udid = simulatorUdid();
  let bridge = null;
  let pressTimeouts = 0;
  const ad = (...a) => run(AGENT_DEVICE, [...a, '--session', AD_SESSION, '--platform', 'ios', '--udid', udid], { cwd: join(HERE, '../..') });
  const post = async (path, body) => (await fetch(`http://127.0.0.1:${RWI_PORT}${path}`, { method: 'POST', body: JSON.stringify(body) })).json();
  return {
    platform: 'ios',
    udid,
    simctl: (...a) => run('xcrun', ['simctl', ...a]),
    async connect() {
      if (!bridge) {
        bridge = spawn('python3', ['-I', join(HERE, 'rwi.py'), 'serve', '--udid', udid, '--port', String(RWI_PORT), '--bundle', BUNDLE], { stdio: 'ignore' });
        for (let i = 0; i < 40; i += 1) {
          try { await post('/reset', {}); break; } catch { await sleep(POLL_MS); }
        }
      } else await post('/reset', {});
      ad('open', BUNDLE);
    },
    async eval(expr) {
      for (let attempt = 0; ; attempt += 1) {
        const r = await post('/eval', { expr });
        if (r.error === undefined) return r.value;
        // the inspector drops the page while the app relaunches: reattach once
        if (attempt === 0 && /Timeout|OSError|Connection/.test(r.error)) { await sleep(POLL_MS * 4); continue; }
        throw new Error(r.error);
      }
    },
    /** After the app was killed: the XCTest session still follows the old process, so it is closed (connect opens anew). */
    endSession() { try { ad('close'); } catch { /* no session */ } },
    /**
     * One XCTest press. Under load it can time out on the runner's watchdog and still land later, so it is never simply
     * sent again here: it answers false and the tap looks at the page (and waits longer) before touching again. A runner
     * that timed out twice in a row is started afresh (the app keeps running).
     */
    async touch(x, y) {
      try {
        ad('press', String(Math.round(x)), String(Math.round(y)));
        pressTimeouts = 0;
        return true;
      } catch (e) {
        if (!/timed out|watchdog|still finishing|deadline exceeded/.test(String(e?.stderr ?? e))) throw e;
        pressTimeouts += 1;
        if (pressTimeouts >= 2) { try { ad('close'); } catch { /* already gone */ } ad('open', BUNDLE); pressTimeouts = 0; }
        return false;
      }
    },
    async type(text) { ad('type', text); },
    async hideKeyboard() { ad('keyboard', 'dismiss'); },
    shot(name) { run('xcrun', ['simctl', 'io', udid, 'screenshot', join(evidence, `${name}.png`)], { stdio: 'ignore' }); },
    /** The XCTest accessibility tree (what VoiceOver's tree holds), through agent-device; the raw snapshot is kept. */
    axTree(name) {
      const json = ad('snapshot', '--json', '--raw').toString();
      writeFileSync(join(evidence, `${name}.json`), json);
      return JSON.parse(json).data.nodes.map((n) => ({ type: n.type ?? '', label: n.label ?? '', hint: '', value: n.value === undefined ? '' : String(n.value), checkable: false, checked: false }));
    },
    close() { try { ad('close'); } catch { /* session already gone */ } bridge?.kill(); },
  };
}

// ------------------------------------------------------------------------------------------------------------ common

/** The device driver: `ev`, `waitFor`, real `tap`, `fill`, `shot`; `platform` is 'android' or 'ios'. */
export async function device(platform, evidence) {
  mkdirSync(evidence, { recursive: true });
  const d = platform === 'ios' ? await ios(evidence) : await android(evidence);
  const ev = async (expr) => { await d.eval(HELPERS); return d.eval(expr); };
  /**
   * Where to touch `sel`: its centre once it holds still (a sheet still rising, a list still scrolling or loading moves
   * it) and the page's own hit test at that point finds the target (nothing covers it, it is where the page lays it
   * out). `aimed` is false when the hit test never found it within the wait: the centre is touched anyway.
   */
  const aim = async (sel, text) => {
    // a touch can land seconds after it is aimed (an iOS XCTest press takes 1-5 s): wait out the status line's fold
    // (a slow page folds it late: wait for the fold itself, at most STATUS_WAIT_MS, then aim anyway)
    await t.waitFor('window.__dv.statusSettled()', 'the status line folded', STATUS_WAIT_MS).catch(() => { t.stats.statusWaits += 1; });
    let r = null;
    for (let i = 0; i < STILL_READS_MAX; i += 1) {
      await sleep(STILL_POLL_MS);
      const next = await ev(`window.__dv.aim(${arg(sel)}, ${arg(text)})`);
      if (next && !next.hit && next.kb) {
        // the keyboard covers the target (a field kept the focus): put it away, as a tap outside the field does
        await ev(`document.activeElement instanceof HTMLElement && document.activeElement.blur(); true`);
        await sleep(KEYBOARD_AWAY_MS);
        await ev(`window.__dv.reveal(${arg(sel)}, ${arg(text)})`);
        r = null;
        continue;
      }
      if (r && next && next.hit && Math.abs(next.x - r.x) < 1 && Math.abs(next.y - r.y) < 1) return { x: next.x, y: next.y, aimed: true };
      r = next ?? r;
    }
    if (!r) throw new Error(`${sel} ${text ?? ''} went away before the tap`);
    return { x: r.x, y: r.y, aimed: false };
  };
  const t = {
    ...d,
    ev,
    /** Real taps made, and every touch that did not reach its target (with what the page saw). */
    stats: { taps: 0, unaimed: 0, statusWaits: 0, pressTimeouts: 0, misses: [] },
    async waitFor(expr, what, ms = WAIT_MS) {
      const end = Date.now() + ms;
      for (;;) {
        if (await ev(`(() => { try { return !!(${expr}); } catch { return false; } })()`).catch(() => false)) return;
        if (Date.now() > end) throw new Error(`timed out (${ms} ms) waiting for ${what}`);
        await sleep(POLL_MS);
      }
    },
    /** A real touch at the centre of the last visible `sel` (whose label or text matches `text`). */
    async tap(sel, text) {
      await t.waitFor(`window.__dv.q(${arg(sel)}, ${arg(text)})`, `${sel} ${text ?? ''}`);
      await ev(`window.__dv.reveal(${arg(sel)}, ${arg(text)})`);
      t.stats.taps += 1;
      // a touch that does not reach its target is made again (TAP_TRIES); the page records whether the target got its click
      for (let attempt = 1; ; attempt += 1) {
        // aimed afresh before every touch: the target can move after it was measured (IDpjU: the pinned row moved when
        // the phone status line folded after a save, and every retry went to the old point)
        const { x, y, aimed } = await aim(sel, text);
        if (!aimed) t.stats.unaimed += 1;
        const since = await ev(`(() => { window.__dv.clicked = false;
          window.__dv.q(${arg(sel)}, ${arg(text)})?.addEventListener('click', () => { window.__dv.clicked = true; }, { capture: true, once: true });
          return window.__dv.now(); })()`);
        const touchedAt = Date.now();
        const sent = await d.touch(x, y);
        const touchMs = Date.now() - touchedAt;
        // a press that timed out may still land: give it longer before the target is aimed and touched again
        if (!sent) t.stats.pressTimeouts += 1;
        const end = Date.now() + (sent ? CLICK_WAIT_MS : LATE_PRESS_WAIT_MS);
        let clicked = false;
        while (!clicked && Date.now() < end) { await sleep(POLL_MS); clicked = await ev('window.__dv.clicked === true').catch(() => true); }
        // the click can also take the target away (a sheet closes): a missing target counts as clicked
        if (clicked || !(await ev(`window.__dv.q(${arg(sel)}, ${arg(text)}) !== null`).catch(() => false))) break;
        // a touch that did not reach its target: what the page received and how it looked
        const probe = await ev(`window.__dv.probe(${x}, ${y}, ${since}, ${arg(sel)}, ${arg(text)})`).catch((e) => ({ error: String(e) }));
        t.stats.misses.push({ target: `${sel} ${text ?? ''}`.trim(), attempt, at: { x: Math.round(x), y: Math.round(y) }, aimed, touchMs, ...probe });
        if (attempt >= TAP_TRIES) throw new Error(`the touch on ${sel} ${text ?? ''} never reached it`);
      }
      await sleep(AFTER_TAP_MS);
    },
    async fill(sel, value) { await t.waitFor(`document.querySelector(${arg(sel)})`, sel); await ev(`window.__dv.fill(${arg(sel)}, ${arg(value)})`); await sleep(POLL_MS); },
    text: (sel) => ev(`window.__dv.text(${arg(sel)})`),
    value: (sel) => ev(`window.__dv.value(${arg(sel)})`),
    async waitValue(sel, want) { await t.waitFor(`document.querySelector(${arg(sel)})?.value === ${arg(want)}`, `${sel} = ${want}`); },
  };
  return t;
}

// ---------------------------------------------------------------------------------------------------- build identity

/** Modified time (s) of a build product. */
export const builtAt = (path) => statSync(path).mtimeMs / 1000;
export const sha256 = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
export const head = () => run('git', ['-C', APP_ROOT, 'rev-parse', 'HEAD']).toString().trim();
/** The commit time (s) of the newest commit that touched the app's sources (the e2e folder does not count). */
export const sourceCommitTime = () => Number(run('git', ['-C', APP_ROOT, 'log', '-1', '--format=%ct', '--', 'ui', 'src-tauri', 'crates']).toString().trim());
export const dirty = () => run('git', ['-C', APP_ROOT, 'status', '--porcelain', '--untracked-files=no', '--', 'ui', 'src-tauri', 'crates']).toString().trim();

export function androidApk() {
  const apk = process.env.E2E_ANDROID_APK ?? join(APP_ROOT, 'src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk');
  if (!existsSync(apk)) throw new Error(`no debug APK at ${apk}: build it (docs/creator-device-e2e.md)`);
  return apk;
}

export function iosApp() {
  const app = process.env.E2E_IOS_APP ?? join(APP_ROOT, 'src-tauri/gen/apple/build/arm64-sim/Borealis.app');
  if (!existsSync(app)) throw new Error(`no simulator build at ${app}: build it (docs/creator-device-e2e.md)`);
  return app;
}
