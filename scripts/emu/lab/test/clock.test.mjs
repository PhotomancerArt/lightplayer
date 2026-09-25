// The lab's clock seam (clock.mjs): the real clock is node's, and the manual
// clock fires exactly the timers a real one would have, at their own due
// times, only when told to move.
'use strict';

import { test } from 'node:test';
import assert from 'node:assert/strict';

import { realClock, createManualClock } from '../clock.mjs';

test('the real clock is Date.now and node\'s timers', async () => {
  assert.equal(realClock.manual, false);
  const before = Date.now();
  const n = realClock.now();
  assert.ok(n >= before && n <= Date.now());
  const fired = await new Promise((resolve) => { realClock.setTimeout(() => resolve(true), 1); });
  assert.equal(fired, true);
});

test('manual time stands still until advanced, and a timeout fires once, at its due time', () => {
  const c = createManualClock(1000);
  const seen = [];
  c.setTimeout(() => seen.push(c.now()), 50);
  assert.equal(c.now(), 1000);
  assert.equal(c.advance(49), 0);
  assert.deepEqual(seen, []);
  assert.equal(c.now(), 1049);
  assert.equal(c.advance(1), 1);
  assert.deepEqual(seen, [1050], 'now() reads the timer\'s own due time');
  c.advance(1000);
  assert.deepEqual(seen, [1050], 'a timeout fires once');
  assert.equal(c.pending(), 0);
});

test('an interval stepped across several periods fires at each, in order with other timers', () => {
  const c = createManualClock(0);
  const seen = [];
  const h = c.setInterval(() => seen.push('i@' + c.now()), 20);
  c.setTimeout(() => seen.push('t@' + c.now()), 40);
  assert.equal(typeof h.unref, 'function', 'handles answer unref() like node\'s');
  assert.equal(h.unref(), h);
  c.advance(100);
  // Ties go to the timer set first: the interval, then the timeout.
  assert.deepEqual(seen, ['i@20', 'i@40', 't@40', 'i@60', 'i@80', 'i@100']);
  c.clearInterval(h);
  c.advance(100);
  assert.equal(seen.length, 6, 'a cleared interval never fires again');
});

test('a timer set from inside a timer fires in the same advance when it falls due inside it', () => {
  const c = createManualClock(0);
  const seen = [];
  c.setTimeout(() => { seen.push(c.now()); c.setTimeout(() => seen.push(c.now()), 10); }, 10);
  c.advance(25);
  assert.deepEqual(seen, [10, 20]);
  assert.equal(c.now(), 25);
});

test('advance refuses a negative or non-numeric step; a clock refuses a non-finite start', () => {
  const c = createManualClock(0);
  assert.throws(() => c.advance(-1), /finite ms >= 0/);
  assert.throws(() => c.advance('soon'), /finite ms >= 0/);
  assert.throws(() => createManualClock(NaN), /finite start/);
});
