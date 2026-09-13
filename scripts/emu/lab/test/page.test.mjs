// P2: the page's pure parts — the row record, the taint reducer, the payload
// builder and the plan builder — under node with no DOM.
'use strict';

import { test } from 'node:test';
import assert from 'node:assert/strict';

import { newRowRecord, noteVisibility, noteLockRelease, finishRow, buildPayload, planFromRows, defaultDeviceName, boardState } from '../lab-page.js';

const visible = { visibility: 'visible', hasFocus: true, wakeLock: 'active' };

test('a row that stays visible with the lock held is clean', () => {
  const rec = finishRow(newRowRecord(visible), visible);
  assert.equal(rec.tainted, false);
  assert.deepEqual(rec.taintReasons, []);
  assert.equal(rec.visibilityAtStart, 'visible');
  assert.equal(rec.visibilityAtEnd, 'visible');
  assert.equal(rec.hasFocusAtStart, true);
  assert.equal(rec.wakeLock, 'active');
});

test('a hidden interval taints the row even if it ends visible', () => {
  const rec = newRowRecord(visible);
  noteVisibility(rec, 'hidden');
  noteVisibility(rec, 'visible');
  finishRow(rec, visible);
  assert.equal(rec.tainted, true);
  assert.deepEqual(rec.taintReasons, ['hidden']);
});

test('a lock released under the row taints it; a lock never held does not', () => {
  const rec = newRowRecord(visible);
  noteLockRelease(rec);
  finishRow(rec, { ...visible, wakeLock: 'released' });
  assert.deepEqual(rec.taintReasons, ['lock-released']);
  assert.equal(rec.wakeLock, 'released');
  for (const never of ['none', 'unsupported', 'denied']) {
    const r = finishRow(newRowRecord({ ...visible, wakeLock: never }), { ...visible, wakeLock: never });
    assert.equal(r.tainted, false, never);
    assert.equal(r.wakeLock, never);
  }
});

test('a row that ends hidden or started hidden is tainted', () => {
  const ended = finishRow(newRowRecord(visible), { ...visible, visibility: 'hidden' });
  assert.deepEqual(ended.taintReasons, ['hidden']);
  const started = finishRow(newRowRecord({ ...visible, visibility: 'hidden' }), visible);
  assert.deepEqual(started.taintReasons, ['hidden']);
});

test('the payload keeps the legacy keys and adds the lab keys; a press is tainted if any row is', () => {
  const manifest = { build: { short: 'abc1234', id: 'abc1234' }, defaults: {} };
  const rows = [
    { slug: 'render-basic', grade: 't2', mode: 'jit', fnBlocks: 8, realtime: 1.0, tainted: false, taintReasons: [] },
    { slug: 'render-basic', grade: 't2', mode: 'interp', fnBlocks: null, realtime: 0.6, tainted: true, taintReasons: ['hidden'] },
  ];
  const p = buildPayload({
    ua: 'UA', cores: 4, at: '2026-09-11T00:00:00.000Z', manifest, deviceMemory: undefined, results: rows, preset: 'gate-rows',
    device: 'd1', deviceName: 'phone', job: 'j-1', press: 3, buildId: 'abc1234', manual: false, state: visible, deferredMs: 0,
  });
  // The legacy shape (what the rig's send() writes), name for name.
  assert.deepEqual(Object.keys(p).slice(0, 6), ['ua', 'cores', 'at', 'build', 'deviceMemory', 'results']);
  assert.equal(p.deviceMemory, null);
  assert.equal(p.preset, 'gate-rows');
  assert.equal(p.build.short, 'abc1234');
  assert.equal(p.device, 'd1');
  assert.equal(p.job, 'j-1');
  assert.equal(p.press, 3);
  assert.equal(p.buildId, 'abc1234');
  assert.equal(p.manual, false);
  assert.equal(p.tainted, true);
  assert.deepEqual(p.taintReasons, ['hidden']);
  assert.equal(p.wakeLock, 'active');
  assert.equal(p.visibility, 'visible');

  const clean = buildPayload({ ua: 'UA', cores: 4, at: 'x', manifest, results: [rows[0]], device: 'd1', manual: true, state: visible });
  assert.equal(clean.tainted, false);
  assert.equal(clean.manual, true);
  assert.equal('preset' in clean, false);

  const dead = buildPayload({ ua: 'UA', cores: 4, at: 'x', manifest, results: [], device: 'd1', state: visible, failed: 'Worker died' });
  assert.equal(dead.tainted, true);
  assert.equal(dead.failed, 'Worker died');
});

test('an explicit row list becomes a plan the way the rig builds one', () => {
  const plan = planFromRows(
    [{ slug: 'render-basic', grade: 't2', mode: 'jit', fnBlocks: 8 }, { slug: 'render-basic', grade: 't2', mode: 'interp' }],
    { wallTimeout: 300, exitOn: true, timeout: '20s', fnBlocks: 32 },
  );
  assert.deepEqual(plan, [
    { slug: 'render-basic', grade: 't2', mode: 'jit', fnBlocks: 8, timeout: '20s', wallTimeout: 300, exitOn: true },
    { slug: 'render-basic', grade: 't2', mode: 'interp', fnBlocks: null, timeout: '20s', wallTimeout: 300, exitOn: true },
  ]);
  assert.equal(planFromRows([{ slug: 'x', grade: 't1', mode: 'jit' }], {})[0].fnBlocks, 32);
  assert.equal(planFromRows([{ slug: 'x', grade: 't1', mode: 'jit' }], {})[0].timeout, '5500ms');
});

test('the default device name comes from the UA, not from a placeholder', () => {
  assert.equal(defaultDeviceName('Mozilla/5.0 (iPhone; CPU iPhone OS 18_7 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.6.1 Mobile/15E148 Safari/604.1'), 'iPhone');
  assert.equal(defaultDeviceName('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15'), 'Mac Safari');
  assert.equal(defaultDeviceName('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/152.0.0.0 Safari/537.36'), 'Mac Chrome');
  assert.equal(defaultDeviceName('Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/128.0 Mobile Safari/537.36'), 'Android');
  assert.equal(defaultDeviceName(''), 'browser');
});

test('the board leads with one state: refused > reconnecting > running > cooldown > waiting > done > idle', () => {
  const base = { joined: true, connected: true, pendingHidden: null, running: null, cooldown: null, queued: 0, lastHeadline: null, now: 0 };
  assert.equal(boardState({ ...base, joined: false }).kind, 'off');
  assert.equal(boardState(base).kind, 'idle');
  assert.equal(boardState({ ...base, lastHeadline: '0.988× real time' }).kind, 'done');
  assert.equal(boardState({ ...base, queued: 3 }).kind, 'waiting');
  assert.match(boardState({ ...base, queued: 3 }).text, /3 presses queued/);
  const cd = boardState({ ...base, queued: 3, cooldown: { job: 'j-1', nextPressAt: new Date(161000).toISOString() } });
  assert.equal(cd.kind, 'cooldown');
  assert.match(cd.text, /next press in 2:41/);
  const run = boardState({ ...base, queued: 3, cooldown: { nextPressAt: 'x' }, running: { press: 4, of: 10, build: '23a3d3c', row: { i: 2, n: 4, what: 'render-basic t2 translated fn 16' } } });
  assert.equal(run.kind, 'running');
  assert.match(run.text, /Running press 4 of 10 · 23a3d3c · row 2 of 4/);
  assert.equal(run.progress, 0.25);
  assert.equal(boardState({ ...base, running: { manual: true, build: 'x', row: null } }).text, 'Running manual · x · loading the build');
  assert.equal(boardState({ ...base, connected: false, running: { press: 1, of: 1, build: 'x' } }).kind, 'off');
  const ref = boardState({ ...base, connected: false, pendingHidden: { press: 2 } });
  assert.equal(ref.kind, 'refused');
  assert.match(ref.text, /press 2 received while the tab is hidden/);
});
