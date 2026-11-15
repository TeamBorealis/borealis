import assert from 'node:assert/strict';
import { test } from 'node:test';
import { isMainThreadTimeout, recoverWedgedOverlay, wedgedOverlayNotice, type OverlayDeps } from './overlay.ts';

/** Fake process table: `alive` answers accessibility, the rest are wedged. Records what was killed. */
function fakeDeps(pids: readonly number[], alive: readonly number[]): OverlayDeps & { killed: number[] } {
  const killed: number[] = [];
  return {
    killed,
    findPids: async () => pids,
    responsive: async (asked) => new Set(asked.filter((pid) => alive.includes(pid))),
    kill: async (pid) => { killed.push(pid); },
  };
}

test('kills an overlay that does not answer accessibility', async () => {
  const deps = fakeDeps([69989], []);
  assert.deepEqual(await recoverWedgedOverlay(deps), [69989]);
  assert.deepEqual(deps.killed, [69989]);
});

test('leaves a responsive overlay alone', async () => {
  const deps = fakeDeps([42046], [42046]);
  assert.deepEqual(await recoverWedgedOverlay(deps), []);
  assert.deepEqual(deps.killed, []);
});

test('kills only the wedged one of several overlays', async () => {
  const deps = fakeDeps([1, 2, 3], [2]);
  assert.deepEqual(await recoverWedgedOverlay(deps), [1, 3]);
  assert.deepEqual(deps.killed, [1, 3]);
});

test('does nothing, and asks nothing, when no overlay runs', async () => {
  const deps = { ...fakeDeps([], []), responsive: async () => assert.fail('no pids to probe') };
  assert.deepEqual(await recoverWedgedOverlay(deps), []);
});

test("recognises agent-device's runner timeout and nothing else", () => {
  assert.equal(isMainThreadTimeout(new Error('main thread execution timed out')), true);
  assert.equal(isMainThreadTimeout('actions failed: Main thread execution timed out'), true);
  assert.equal(isMainThreadTimeout(new Error('text entry verification failed')), false);
});

test('the notice names the overlay and the killed pids', () => {
  assert.match(wedgedOverlayNotice([69989]), /AutomationModeUI \(pid 69989\).*killed/);
});
