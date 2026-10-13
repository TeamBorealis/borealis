import { execFile, spawn, type ChildProcess } from 'node:child_process';
import { mkdirSync, existsSync, renameSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, dirname } from 'node:path';
import { promisify } from 'node:util';
import { createAgentDeviceClient, type AgentDeviceClient } from 'agent-device';
import { ConfigurationError, EngineError, defineEngine, resolveExpression, type EngineHandle, type SemanticNode } from 'e2e/engine';
import { hideOtherCopies, showCopies } from './launch-services.ts';
import { backupAndClear, resetData, restore, seedAiSetupDone } from './data-dir.ts';
import { toChord } from './keys.ts';
import { clickAt, permissionMessage, probePermissions, sendKeyCode } from './native.ts';
import { assertBundleFresh, clearStaleClaims } from './fresh.ts';
import { projectSnapshot, type RawNode } from './project.ts';

const exec = promisify(execFile);
// agent-device's Apple runner needs full Xcode, not the Command Line Tools.
if (!process.env['DEVELOPER_DIR']) process.env['DEVELOPER_DIR'] = '/Applications/Xcode.app/Contents/Developer';
const PLATFORM = 'macos' as const;
const ROOT_ID = 'root';
const LAUNCH_TIMEOUT_MS = 45_000;
const QUIT_GRACE_MS = 6_000;

export interface MacosOptions { readonly session?: string }

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const fail = (code: 'INVALID_STATE' | 'ENGINE_FAILURE' | 'NODE_STALE' | 'NOT_ACTIONABLE' | 'UNSUPPORTED_CAPABILITY', msg: string, retryable = false) =>
  new EngineError(code, msg, { retryable });

/** Engine for the real macOS Borealis.app, driven through agent-device's macOS helper. One worker (one GUI). */
export function macos(options: MacosOptions = {}): EngineHandle {
  let client: AgentDeviceClient | undefined;
  let appPath = '';
  let bundleId = 'app.borealis';
  let counter = 0;
  let refs = new Map<string, string>();
  let centers = new Map<string, { x: number; y: number }>();
  let origin = { x: 0, y: 0 };
  let viewport = { width: 1200, height: 800 };
  let artifactsDir = '';
  let shots = 0;
  let recorder: { child?: ChildProcess; path: string; startedAt: string; viaHelper: boolean } | undefined;
  const session = options.session ?? `borealis-e2e-${process.pid}`;
  const home = homedir();
  let hidden: readonly string[] = [];
  const onExit = () => { try { restore(home); } catch { /* best effort; next run recovers from the marker */ } };

  const c = (): AgentDeviceClient => (client ??= createAgentDeviceClient({ session }));
  const sel = { platform: PLATFORM } as const;

  async function running(): Promise<boolean> {
    try { await exec('pgrep', ['-f', join(appPath, 'Contents/MacOS/')]); return true; } catch { return false; }
  }
  async function quit(): Promise<void> {
    await c().apps.close({ ...sel, app: bundleId }).catch(() => undefined);
    await c().sessions.close().catch(() => undefined);
    const deadline = Date.now() + QUIT_GRACE_MS;
    while (Date.now() < deadline && (await running())) await sleep(200);
    if (await running()) {
      await exec('pkill', ['-TERM', '-f', join(appPath, 'Contents/MacOS/')]).catch(() => undefined);
      await sleep(1_000);
    }
    if (await running()) await exec('pkill', ['-KILL', '-f', join(appPath, 'Contents/MacOS/')]).catch(() => undefined);
  }
  async function snapshot(interactiveOnly = false): Promise<{ raw: RawNode[]; app?: string }> {
    const r = await c().capture.snapshot({ ...sel, interactiveOnly });
    if (process.env['E2E_MACOS_DUMP']) writeFileSync(process.env['E2E_MACOS_DUMP'], JSON.stringify(r.nodes, null, 1));
    return { raw: r.nodes as unknown as RawNode[], ...(r.appBundleId ?? r.appName ? { app: (r.appBundleId ?? r.appName)! } : {}) };
  }
  async function launch(): Promise<void> {
    if (await running()) await quit();
    await exec('open', ['-a', appPath]);
    // /Applications/Borealis-old.app shares the bundle id: let this build register as running first, else open-by-id starts that copy.
    for (let i = 0; i < 50 && !(await running()); i += 1) await sleep(200);
    await sleep(1_000);
    await c().apps.open({ ...sel, app: bundleId });
    const deadline = Date.now() + LAUNCH_TIMEOUT_MS;
    for (;;) {
      const { raw } = await snapshot().catch(() => ({ raw: [] as RawNode[] }));
      if (raw.length > 5) return;
      if (Date.now() > deadline) throw fail('INVALID_STATE', `Borealis showed no accessibility tree within ${LAUNCH_TIMEOUT_MS} ms`);
      await sleep(500);
    }
  }
  async function project(): Promise<ReturnType<typeof projectSnapshot> & { app?: string }> {
    const { raw, app } = await snapshot();
    const p = projectSnapshot(raw, () => `n${++counter}`);
    refs = p.refs;
    centers = p.centers;
    origin = p.origin;
    viewport = p.viewport;
    return { ...p, ...(app === undefined ? {} : { app }) };
  }
  const target = (id: string) => {
    const ref = refs.get(id);
    if (ref === undefined) throw fail('NODE_STALE', `node ${id} is not part of the newest snapshot`, true);
    return { ref };
  };
  async function click(id: string, count = 1): Promise<void> {
    target(id); // NODE_STALE when the node is not in the newest snapshot
    const at = centers.get(id);
    if (at === undefined) throw fail('NOT_ACTIONABLE', `node ${id} has no on-screen rect`);
    await clickAt(at.x, at.y, count);
    await sleep(250);
  }
  async function chord(key: string): Promise<void> {
    const k = toChord(key);
    if (k === undefined) throw fail('UNSUPPORTED_CAPABILITY', `cannot send key "${key}" on macOS`);
    await exec('open', ['-a', appPath]); // keys go to the frontmost app
    await sendKeyCode(k.code, k.modifiers);
  }
  async function typeInto(text: string, replace: boolean): Promise<void> {
    if (replace) { await chord('ControlOrMeta+a'); await chord(text === '' ? 'Backspace' : 'Delete'); }
    if (text !== '') await c().interactions.type({ ...sel, text });
  }

  return defineEngine({
    name: 'borealis-macos',
    version: '1.0.0',
    spiVersion: 1,
    platform: 'macos' as never,
    workers: 1,
    validateApp(app, { targetName }) {
      if (app.appPath === undefined) throw new ConfigurationError('INVALID_CONFIG', `target "${targetName}" needs app.appPath (the built Borealis.app)`);
    },
    async prepare(info) {
      appPath = info.app.appPath ? join(info.projectRoot, info.app.appPath) : '';
      if (!existsSync(appPath)) throw new ConfigurationError('INVALID_CONFIG', `Borealis.app not found at ${appPath}; build it with \`npm run tauri -- build\` (docs/build.md)`);
      await assertBundleFresh(appPath, info.projectRoot);
      clearStaleClaims(home, session);
      const perms = await probePermissions();
      if (!perms.accessibility || !perms.screenRecording) {
        throw new ConfigurationError('INVALID_CONFIG', permissionMessage(perms)); // before anything is touched
      }
      hidden = await hideOtherCopies(appPath, bundleId);
      info.log(`hid ${hidden.length} other installed copy(ies) of ${bundleId} from Launch Services for the run`);
      const { moved } = backupAndClear(info.runId, home);
      info.log(`backed up ${moved} Borealis data location(s) to ~/.borealis-e2e-backup; restored on finish`);
      process.once('exit', onExit);
      return {};
    },
    async finish(info) {
      client && (await c().sessions.close().catch(() => undefined));
      if (appPath) await quit().catch(() => undefined);
      await showCopies(hidden);
      const { restored } = restore(home);
      process.off('exit', onExit);
      info.log?.(`restored ${restored} Borealis data location(s)`);
    },
    async init(info) {
      appPath = join(info.projectRoot, info.app.appPath!);
      bundleId = info.app.bundleId ?? bundleId;
    },
    async startAttempt(ctx) {
      artifactsDir = ctx.artifactsDir;
      shots = 0;
      await quit();
      resetData(home); // throws unless the user's data is safely backed up
      seedAiSetupDone(home); // the first-run "Set up AI assistance" screen would cover every suite
    },
    async endAttempt() {
      if (recorder) await stopRecording().catch(() => undefined);
      await quit().catch(() => undefined);
    },
    async dispose() { await quit().catch(() => undefined); client = undefined; },

    async observe() {
      const p = await project();
      return {
        root: { ref: { id: ROOT_ID, revision: '' }, role: 'window', children: p.roots } as SemanticNode,
        viewport: p.viewport,
        ...(p.app === undefined ? {} : { location: p.app }),
      };
    },
    async locate(expression) {
      const p = await project();
      return resolveExpression(expression, p.roots);
    },
    actions: ['tap', 'doubleTap', 'fill', 'clear', 'press', 'focus', 'swipe'],
    async perform(ref, action) {
      if (action.kind === 'swipe') {
        const direction = ({ up: 'down', down: 'up', left: 'right', right: 'left' } as const)[action.direction as 'up'] ?? 'down';
        await c().interactions.scroll({ ...sel, direction });
        return;
      }
      await exec('open', ['-a', appPath]);
      switch (action.kind) {
        case 'tap': case 'focus': await click(ref.id); return;
        case 'doubleTap': await click(ref.id, 2); return;
        case 'fill': case 'clear':
          // Real keystrokes (not AX value writes) so the webview's input events fire.
          await click(ref.id);
          await typeInto(action.kind === 'fill' ? action.value : '', true);
          return;
        case 'press': await chord(action.key); return; // keys go to the frontmost app's focus; no click, so shortcuts do not hit a control
        default: throw fail('UNSUPPORTED_CAPABILITY', `action ${(action as { kind: string }).kind} is not supported on macOS`);
      }
    },
    async performAt(point, action) {
      await exec('open', ['-a', appPath]);
      const at = { x: Math.round(origin.x + point.x), y: Math.round(origin.y + point.y) };
      await clickAt(at.x, at.y, action.kind === 'doubleTap' ? 2 : 1);
    },
    pointerActions: ['tap', 'doubleTap'],
    keyboard: {
      type: async (text, opts) => typeInto(text, opts.replace === true),
      press: async (key) => chord(key),
    },
    session: {
      restart: async () => { await launch(); },
      reset: async () => { await quit(); resetData(home); await launch(); },
    },
    artifacts: {
      async screenshot(label) {
        shots += 1;
        const rel = join('screenshots', `${String(shots).padStart(3, '0')}-${(label ?? 'screenshot').replace(/[^\w.-]+/g, '_')}.png`);
        mkdirSync(join(artifactsDir, 'screenshots'), { recursive: true });
        await c().capture.screenshot({ ...sel, path: join(artifactsDir, rel) });
        return rel;
      },
      async startVideo() {
        const rel = join('video', 'video.mp4');
        const path = join(artifactsDir, rel);
        mkdirSync(dirname(path), { recursive: true });
        // Whole-screen capture shows the user's desktop around the window; the collect script crops it with window.json.
        recorder = { path, startedAt: new Date().toISOString(), viaHelper: false };
        recorder.child = spawn('screencapture', ['-v', '-x', path.replace(/\.mp4$/, '.mov')], { stdio: 'ignore' });
      },
      async stopVideo() { return recorder ? await stopRecording() : []; },
    },
  });

  async function writeWindowInfo(videoPath: string): Promise<void> {
    await project().catch(() => undefined);
    const bounds = await exec('osascript', ['-e', 'tell application "Finder" to get bounds of window of desktop']).then((r) => r.stdout.trim().split(/,\s*/).map(Number)).catch(() => [0, 0, 0, 0]);
    writeFileSync(join(dirname(videoPath), 'window.json'), JSON.stringify({ x: origin.x, y: origin.y, width: viewport.width, height: viewport.height, screenWidth: bounds[2], screenHeight: bounds[3] }));
  }

  async function stopRecording() {
    const r = recorder!;
    await writeWindowInfo(r.path); // while the app is still on screen
    recorder = undefined;
    let path = r.path;
    if (r.viaHelper) await c().recording.record({ action: 'stop' });
    else if (r.child) {
      r.child.kill('SIGINT');
      await new Promise((res) => r.child!.once('exit', res));
      const mov = r.path.replace(/\.mp4$/, '.mov');
      if (existsSync(mov)) { renameSync(mov, r.path); path = r.path; }
    }
    return existsSync(path) ? [{ path: join('video', 'video.mp4'), startedAt: r.startedAt }] : [];
  }
}
