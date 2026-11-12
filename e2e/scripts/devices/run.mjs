#!/usr/bin/env node
// The creator's device suite (spec 18.4 flow 10, 18.5 phone budget). Usage, from e2e/:
//   node scripts/devices/run.mjs <android|ios> [--no-install] [--allow-stale] [--only flow1,flow3,flow4,flow8,budget]
// Builds are not made here (docs/creator-device-e2e.md). Evidence and results.json go to
// ../target/creator-devices/<platform>/ (the worktree's target folder, git-ignored).

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { APP_ROOT, device } from './driver.mjs';
import { budget, flow1, flow3, flow4, flow8, PHONE_SAVE_BUDGET_MS } from './flows.mjs';
import { freshInstall, launch } from './install.mjs';

const [platform, ...rest] = process.argv.slice(2);
if (platform !== 'android' && platform !== 'ios') {
  console.error('usage: node scripts/devices/run.mjs <android|ios> [--no-install] [--only flow1,flow3,...]');
  process.exit(2);
}
const only = rest.includes('--only') ? rest[rest.indexOf('--only') + 1].split(',') : null;
const out = join(process.env.E2E_DEVICE_OUT ?? join(APP_ROOT, 'target/creator-devices'), platform);
mkdirSync(out, { recursive: true });

const STEPS = [['flow1', flow1], ['flow3', flow3], ['flow4', flow4], ['flow8', flow8], ['budget', budget]];
const d = await device(platform, out);
const result = { platform, started: new Date().toISOString(), identity: null, steps: [], saves: [] };
let failed = false;
try {
  result.identity = rest.includes('--no-install') ? null : await freshInstall(d, { allowStale: rest.includes('--allow-stale') });
  if (!result.identity) await launch(d);
  console.log('identity', JSON.stringify(result.identity));
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
  result.budget = { limitMs: PHONE_SAVE_BUDGET_MS, maxMs: Math.max(0, ...result.saves.map((s) => s.ms)), over };
  if (over.length > 0) failed = true;
  writeFileSync(join(out, 'results.json'), JSON.stringify(result, null, 1));
  console.log('saves', JSON.stringify(result.saves));
  console.log(failed ? 'RESULT: FAIL' : 'RESULT: PASS');
  d.close();
}
process.exit(failed ? 1 : 0);
