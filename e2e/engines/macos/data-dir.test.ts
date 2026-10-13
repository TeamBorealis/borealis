import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, test } from 'node:test';
import { backupAndClear, hasBackup, MANAGED_PATHS, resetData, restore } from './data-dir.ts';

let home = '';
const data = () => join(home, MANAGED_PATHS[0]);
const seed = (text: string) => { mkdirSync(join(data(), 'characters'), { recursive: true }); writeFileSync(join(data(), 'characters', 'hero.json'), text); };
const read = () => readFileSync(join(data(), 'characters', 'hero.json'), 'utf8');

beforeEach(() => { home = mkdtempSync(join(tmpdir(), 'borealis-home-')); });
afterEach(() => { rmSync(home, { recursive: true, force: true }); });

test('backs up real data, lets tests wipe it, and restores it byte for byte', () => {
  seed('REAL');
  assert.equal(backupAndClear('r1', home).moved, 1);
  assert.equal(existsSync(data()), false);
  mkdirSync(data(), { recursive: true });
  writeFileSync(join(data(), 'test.txt'), 'TEST');
  resetData(home);
  assert.equal(existsSync(data()), false);
  mkdirSync(data(), { recursive: true });
  writeFileSync(join(data(), 'leftover'), 'x');
  assert.equal(restore(home).restored, 1);
  assert.equal(read(), 'REAL');
  assert.equal(existsSync(join(data(), 'leftover')), false);
  assert.equal(hasBackup(home), false);
});

test('a crashed run is recovered by the next one before anything else happens', () => {
  seed('REAL');
  backupAndClear('crashed', home);
  mkdirSync(data(), { recursive: true });
  writeFileSync(join(data(), 'junk'), 'x');
  backupAndClear('next', home); // must restore REAL first, then move it aside again
  assert.equal(existsSync(data()), false);
  restore(home);
  assert.equal(read(), 'REAL');
});

test('resetData refuses without a backup marker', () => {
  seed('REAL');
  assert.throws(() => resetData(home), /no backup marker/);
  assert.equal(read(), 'REAL');
});

test('with no user data, restore leaves none behind', () => {
  assert.equal(backupAndClear('r', home).moved, 0);
  mkdirSync(data(), { recursive: true });
  restore(home);
  assert.equal(existsSync(data()), false);
  assert.equal(restore(home).restored, 0); // idempotent
});
