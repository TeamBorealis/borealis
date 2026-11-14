#!/usr/bin/env node
// The creator's device suite (spec 18.4 flow 10, 18.5 phone budget). Usage, from e2e/:
//   node scripts/devices/run.mjs <android|ios> [--no-install] [--allow-stale] [--library <dir>]
//     [--only flow1,flow3,flow4,flow8,budget,spellLists,cellMarks,a11y,share]
// Builds are not made here (docs/creator-device-e2e.md). Evidence and results.json go to
// ../target/creator-devices/<platform>/ (the worktree's target folder, git-ignored).

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { APP_ROOT, device } from './driver.mjs';
import { a11y, cellMarks, share, spellLists } from './checks.mjs';
import { budget, flow1, flow3, flow4, flow8, PHONE_SAVE_BUDGET_MS } from './flows.mjs';
import { freshInstall, launch, loadedContent } from './install.mjs';

const [platform, ...rest] = process.argv.slice(2);
if (platform !== 'android' && platform !== 'ios') {
  console.error('usage: node scripts/devices/run.mjs <android|ios> [--no-install] [--only flow1,flow3,...]');
  process.exit(2);
}
const only = rest.includes('--only') ? rest[rest.indexOf('--only') + 1].split(',') : null;
// a large library installed with the app (a folder written by src-tauri/tests/device_library.rs), for the budget
const library = rest.includes('--library') ? rest[rest.indexOf('--library') + 1] : null;
const out = join(process.env.E2E_DEVICE_OUT ?? join(APP_ROOT, 'target/creator-devices'), platform);
mkdirSync(out, { recursive: true });

const STEPS = [['flow1', flow1], ['flow3', flow3], ['flow4', flow4], ['flow8', flow8], ['budget', budget],
  ['spellLists', spellLists], ['cellMarks', cellMarks], ['a11y', a11y], ['share', share]];
const d = await device(platform, out);
const result = { platform, started: new Date().toISOString(), identity: null, library, content: null, steps: [], saves: [], checks: [] };
let failed = false;
try {
  result.identity = rest.includes('--no-install') ? null : await freshInstall(d, { allowStale: rest.includes('--allow-stale'), library });
  if (!result.identity) await launch(d);
  console.log('identity', JSON.stringify(result.identity));
  result.content = await loadedContent(d);
  console.log('content', JSON.stringify(result.content));
  for (const [name, step] of STEPS) {
    if (only && !only.includes(name)) continue;
    const t0 = Date.now();
    try {
      await step(d, result);
      result.steps.push({ name, ok: true, s: Math.round((Date.now() - t0) / 1000) });
      console.log(`PASS ${name} (${Math.round((Date.now() - t0) / 1000)} s)`);
    } catch (e) {
      failed = true;
      d.shot(`FAIL-${name}`);
      result.steps.push({ name, ok: false, error: String(e?.message ?? e) });
      console.log(`FAIL ${name}: ${e?.message ?? e}`);
      break;
    }
  }
} finally {
  const over = result.saves.filter((s) => s.ms >= PHONE_SAVE_BUDGET_MS);
  result.taps = { taps: d.stats.taps, missed: d.stats.misses.length, unaimed: d.stats.unaimed, misses: d.stats.misses };
  console.log('taps', d.stats.taps, 'missed touches', d.stats.misses.length);
  result.budget = { limitMs: PHONE_SAVE_BUDGET_MS, maxMs: Math.max(0, ...result.saves.map((s) => s.ms)), over };
  if (over.length > 0) failed = true;
  writeFileSync(join(out, 'results.json'), JSON.stringify(result, null, 1));
  console.log('saves', JSON.stringify(result.saves));
  console.log(failed ? 'RESULT: FAIL' : 'RESULT: PASS');
  d.close();
}
process.exit(failed ? 1 : 0);
