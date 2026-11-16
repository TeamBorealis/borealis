// Unit tests of the device driver's pure parts (node --test scripts/devices/driver.test.mjs).
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { iosTouchArgs } from './driver.mjs';

test('a touch on a button, a check box or a row is a zero-distance XCTest pan (no text-input probe)', () => {
  // I8pYk: agent-device's press first probes every rendered text input at the point, three XCTest queries each; on a
  // 55k-element library with Try it open (95 inputs, a 12k-node tree, about 1 s a query) a press took 80 s to land
  assert.deepEqual(iosTouchArgs(296.6, 805.5, false), ['gesture', 'pan', '297', '806', '0', '0', '60']);
});

test('a touch on a text field stays a press, so the runner knows the field it focused for typing', () => {
  assert.deepEqual(iosTouchArgs(120.2, 300.7, true), ['press', '120', '301']);
});
