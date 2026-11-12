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
/** A target must read the same rectangle twice this far apart before it is touched (at most STILL_READS_MAX reads). */
const STILL_POLL_MS = 200;
const STILL_READS_MAX = 25;
/** iOS: tries of one XCTest press, and the pause between them. */
const PRESS_TRIES = 4;
const PRESS_RETRY_MS = 4_000;

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const env = { ...process.env, DEVELOPER_DIR };
const run = (cmd, args, opts = {}) => execFileSync(cmd, args, { env, maxBuffer: 256 * 1024 * 1024, stdio: ["ignore", "pipe", "pipe"], ...opts });

/** Page-side helpers, installed once per page load: query by CSS + text, fill through React's setter, rectangles. */
const HELPERS = `(() => {
  if (window.__dv) return true;
  const visible = (e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0; };
  window.__dv = {
    q(sel, text) {
      const all = [...document.querySelectorAll(sel)].filter(visible);
      if (text === undefined || text === null) return all[all.length - 1] ?? null;
      const re = new RegExp(text);
      return all.filter((e) => re.test((e.getAttribute('aria-label') ?? '').trim()) || re.test((e.textContent ?? '').trim())).pop() ?? null;
    },
    rect(sel, text) { const e = this.q(sel, text); if (!e) return null; const r = e.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height }; },
    reveal(sel, text) { const e = this.q(sel, text); if (!e) return false; e.scrollIntoView({ block: 'center', behavior: 'instant' }); return true; },
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
    },
    /** Real keyboard input into the focused field (adb input text: spaces as %s). */
    async type(text) { adb('shell', 'input', 'text', text.replace(/ /g, '%s')); },
    async hideKeyboard() { adb('shell', 'input', 'keyevent', '111'); },
    shot(name) { writeFileSync(join(evidence, `${name}.png`), adb('exec-out', 'screencap', '-p')); },
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
    async touch(x, y) {
      // XCTest's press can time out on its main-thread watchdog while the machine is loaded: the tap did not happen, so
      // it is tried again (at most PRESS_TRIES times) before the step fails
      for (let attempt = 1; ; attempt += 1) {
        try { ad('press', String(Math.round(x)), String(Math.round(y))); return; } catch (e) {
          if (attempt >= PRESS_TRIES || !/timed out|watchdog|still finishing/.test(String(e?.stderr ?? e))) throw e;
          await sleep(PRESS_RETRY_MS);
        }
      }
    },
    async type(text) { ad('type', text); },
    async hideKeyboard() { ad('keyboard', 'dismiss'); },
    shot(name) { run('xcrun', ['simctl', 'io', udid, 'screenshot', join(evidence, `${name}.png`)], { stdio: 'ignore' }); },
    close() { try { ad('close'); } catch { /* session already gone */ } bridge?.kill(); },
  };
}

// ------------------------------------------------------------------------------------------------------------ common

/** The device driver: `ev`, `waitFor`, real `tap`, `fill`, `shot`; `platform` is 'android' or 'ios'. */
export async function device(platform, evidence) {
  mkdirSync(evidence, { recursive: true });
  const d = platform === 'ios' ? await ios(evidence) : await android(evidence);
  const ev = async (expr) => { await d.eval(HELPERS); return d.eval(expr); };
  const t = {
    ...d,
    ev,
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
      // touch only once the target holds still (a sheet still rising, a list still scrolling or loading moves it)
      let r = null;
      for (let i = 0; i < STILL_READS_MAX; i += 1) {
        await sleep(STILL_POLL_MS);
        const next = await ev(`window.__dv.rect(${arg(sel)}, ${arg(text)})`);
        if (r && next && Math.abs(next.x - r.x) < 1 && Math.abs(next.y - r.y) < 1) { r = next; break; }
        r = next;
      }
      if (!r) throw new Error(`${sel} ${text ?? ''} went away before the tap`);
      await d.touch(r.x + r.w / 2, r.y + r.h / 2);
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
