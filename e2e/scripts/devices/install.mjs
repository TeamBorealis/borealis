// Fresh install with build identity (the emulator rule: prove the installed build is the one just built) and the
// creator switched on in settings.json, for the creator's device suite.

import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { androidApk, BUNDLE, builtAt, dirty, head, iosApp, sha256, sleep, sourceCommitTime } from './driver.mjs';

const SETTINGS = JSON.stringify({ creatorEnabled: true });
/** What a device library folder (`src-tauri/tests/device_library.rs`) holds for the app's data folder. */
const LIBRARY_FILES = ['library', 'content-prefs.json'];
const TAR_MAX_BYTES = 4 * 1024 ** 3;
/** A first launch makes the data folder the settings file goes into. */
const FIRST_LAUNCH_MS = 8_000;
/** adb streams a debug APK of over 1.5 GB for minutes. */
const ADB_INSTALL_TIMEOUT_MS = 10 * 60_000;

/** Uninstall, install the build, check the installed bytes are the built ones, switch the creator on, launch. */
export async function freshInstall(d, { allowStale = false, library = null } = {}) {
  const identity = { head: head(), dirty: dirty() };
  const artifact = d.platform === 'android' ? androidApk() : join(iosApp(), 'Borealis');
  const stale = builtAt(artifact) < sourceCommitTime() ? 'older than the last commit touching ui/, src-tauri/ or crates/'
    : identity.dirty ? 'the app sources have uncommitted changes' : '';
  identity.fresh = stale === '';
  if (stale && !allowStale) throw new Error(`the ${d.platform} build is stale (${stale}): rebuild it (docs/creator-device-e2e.md) or pass --allow-stale`);
  if (d.platform === 'android') Object.assign(identity, await installAndroid(d, library));
  else Object.assign(identity, await installIos(d, library));
  if (identity.built !== identity.installed) throw new Error(`installed build ${identity.installed} is not the built ${identity.built}`);
  return identity;
}

async function installAndroid(d, library) {
  const apk = androidApk();
  const built = sha256(apk);
  try { d.adb('uninstall', BUNDLE); } catch { /* not installed */ }
  execFileSync(d.adbPath, ['-s', d.serial, 'install', '-r', '-t', apk], { timeout: ADB_INSTALL_TIMEOUT_MS, stdio: 'ignore' });
  const remote = d.adb('shell', 'pm', 'path', BUNDLE).toString().split('\n').find((l) => l.endsWith('base.apk'))?.replace('package:', '').trim();
  const local = join(mkdtempSync(join(tmpdir(), 'borealis-apk-')), 'base.apk');
  d.adb('pull', remote, local);
  const installed = sha256(local);
  d.adb('shell', 'am', 'start', '-W', '-n', `${BUNDLE}/.MainActivity`);
  await sleep(FIRST_LAUNCH_MS);
  d.adb('shell', 'am', 'force-stop', BUNDLE);
  execFileSync(d.adbPath, ['-s', d.serial, 'shell', 'run-as', BUNDLE, 'sh', '-c', `'cat > /data/user/0/${BUNDLE}/settings.json'`], { input: SETTINGS });
  if (library) {
    // the library folder and its content preferences, streamed into the app's data folder as the app's user
    const tar = execFileSync('tar', ['-C', library, '-cf', '-', ...LIBRARY_FILES], { maxBuffer: TAR_MAX_BYTES, env: { ...process.env, COPYFILE_DISABLE: '1' } });
    execFileSync(d.adbPath, ['-s', d.serial, 'exec-in', 'run-as', BUNDLE, 'tar', '-xf', '-', '-C', `/data/user/0/${BUNDLE}`], { input: tar, timeout: ADB_INSTALL_TIMEOUT_MS });
  }
  await launch(d);
  return { artifact: apk, built, installed };
}

async function installIos(d, library) {
  const app = iosApp();
  const built = sha256(join(app, 'Borealis'));
  try { d.simctl('uninstall', d.udid, BUNDLE); } catch { /* not installed */ }
  d.simctl('install', d.udid, app);
  const installedApp = d.simctl('get_app_container', d.udid, BUNDLE, 'app').toString().trim();
  const installed = sha256(join(installedApp, 'Borealis'));
  d.simctl('launch', d.udid, BUNDLE);
  await sleep(FIRST_LAUNCH_MS);
  d.simctl('terminate', d.udid, BUNDLE);
  const data = d.simctl('get_app_container', d.udid, BUNDLE, 'data').toString().trim();
  const dir = join(data, 'Library/Application Support', BUNDLE);
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'settings.json'), SETTINGS);
  for (const f of library ? LIBRARY_FILES : []) cpSync(join(library, f), join(dir, f), { recursive: true });
  await launch(d);
  return { artifact: app, built, installed };
}

/** Starts the app (again) and attaches the driver to its page. */
export async function launch(d) {
  if (d.platform === 'android') d.adb('shell', 'am', 'start', '-W', '-n', `${BUNDLE}/.MainActivity`);
  else d.simctl('launch', d.udid, BUNDLE);
  await sleep(FIRST_LAUNCH_MS / 2);
  await d.connect();
  await d.waitFor(`document.querySelector('.aur-window')`, 'the app shell');
}

/** Kills the app as the system does (no chance to save), mid-edit. */
export function kill(d) {
  if (d.platform === 'android') d.adb('shell', 'am', 'force-stop', BUNDLE);
  else {
    d.endSession();
    const pid = d.simctl('spawn', d.udid, 'launchctl', 'list').toString().split('\n').find((l) => l.includes(`UIKitApplication:${BUNDLE}`))?.split('\t')[0];
    // a simulator app is a process of this Mac: killed by its PID, as the system kills an app it reclaims
    if (pid && pid !== '-') process.kill(Number(pid), 'SIGKILL');
    else d.simctl('terminate', d.udid, BUNDLE);
  }
}

/** The packs the running app loaded and their elements (the backend's own count, through the page's Tauri bridge). */
export async function loadedContent(d) {
  // started in the page and read back when it settles (the iOS bridge returns no promise results)
  await d.ev(`(() => { window.__dvContent = null; window.__TAURI_INTERNALS__.invoke('list_packs')
    .then((ps) => { window.__dvContent = { packs: ps.length, elements: ps.reduce((n, p) => n + (p.enabled ? p.elementCount : 0), 0) }; })
    .catch((e) => { window.__dvContent = { error: String(e) }; }); return true; })()`);
  await d.waitFor('window.__dvContent !== null', 'the loaded packs');
  return d.ev('window.__dvContent');
}
