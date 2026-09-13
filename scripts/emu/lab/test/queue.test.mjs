// P3: the queue — spacing from the END of the previous press (D19), A/B
// interleave bound to one device (D20), taint retry (D23), TTL, the device
// filter, waits, restart safety — driven by a fake page client at test speed.
'use strict';

import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { startServer } from './helpers.mjs';
import { fakeDevice } from './fake-device.mjs';

// `LAB_COOLDOWN_MS: '0'` is the cooldown OFF — the ceiling of 0 short-circuits
// the burn-proportional model (C), the way it short-circuited the flat wait
// before it. Every timing assertion in this file below rests on that.
const FAST = { LAB_TICK_MS: '20', LAB_COOLDOWN_MS: '0', LAB_LOST_MS: '400' };

function stageFixture(home, ids) {
  for (const id of ids) {
    fs.mkdirSync(path.join(home, 'builds', id), { recursive: true });
    fs.writeFileSync(path.join(home, 'builds', id, 'manifest.json'), JSON.stringify({
      build: { id, short: id, branch: 'x', dirty: false, built_at: '2026-09-11T00:00:00Z' },
      images: [{ slug: 'render-basic', elf: 'fw-render-basic.elf' }],
      defaults: { wallTimeout: 600, exitOn: false, fnBlocks: 32, timeout: '5500ms' },
    }));
  }
}

let home, lab;
const apiOn = (l, p, opts = {}) => fetch(l.url + p, { ...opts, headers: { Authorization: 'Bearer ' + l.token, 'Content-Type': 'application/json', ...(opts.headers || {}) } });
const api = (p, opts = {}) => apiOn(lab, p, opts);
const queue = async (body) => { const r = await api('/jobs', { method: 'POST', body: JSON.stringify(body) }); return { status: r.status, body: await r.json() }; };

/// A second lab of its own, so a test can pin the two bounds against each
/// other (a wall bound a minute away, a drop window a quarter-second away)
/// without moving them for every other test in the file.
async function ownLab(env) {
  const h = fs.mkdtempSync(path.join(os.tmpdir(), 'emu-lab-drop-'));
  stageFixture(h, ['aaa1111']);
  const l = await startServer(h, env);
  l.queue = async (body) => { const r = await apiOn(l, '/jobs', { method: 'POST', body: JSON.stringify(body) }); return { status: r.status, body: await r.json() }; };
  l.job = async (id) => (await apiOn(l, '/jobs/' + id)).json();
  return l;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
/// Poll a predicate at test speed; throws with the last value on timeout.
async function until(what, fn, timeoutMs = 5000) {
  const end = Date.now() + timeoutMs;
  let last;
  for (;;) {
    last = await fn();
    if (last) return last;
    if (Date.now() > end) throw new Error('timed out waiting for ' + what);
    await sleep(20);
  }
}
const waitJob = async (id, timeout = 10) => { const r = await api('/wait?job=' + id + '&timeout=' + timeout); return { status: r.status, body: await r.json() }; };

before(async () => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), 'emu-lab-queue-'));
  stageFixture(home, ['aaa1111', 'bbb2222']);
  lab = await startServer(home, FAST);
});
after(() => lab && lab.stop());

test('a single-build 3-press job: presses in order, never before spacing from the previous END; report matches', async () => {
  const table = [0.863, 0.945, 1.008];
  const dev = await fakeDevice(lab, { pressMs: 30, numbers: (b, n) => [{ key: 'render-basic/t2/jit/8', realtime: table[Math.ceil(n / 1) - 1] }, { key: 'render-basic/t2/interp', realtime: 0.5 }] });
  const { status, body: j } = await queue({ builds: ['aaa1111'], rows: 'gate-rows', repeats: 3, spacingMs: 150, ttlMs: 60000 });
  assert.equal(status, 201);
  assert.equal(j.presses.length, 3);
  assert.equal(j.state, 'queued');
  const w = await waitJob(j.id);
  assert.equal(w.status, 200);
  assert.equal(w.body.job.state, 'done');
  assert.deepEqual(dev.answered.map((a) => a.press), [1, 2, 3]);
  for (let i = 1; i < dev.answered.length; i++) {
    const gap = dev.answered[i].sentAt - dev.answered[i - 1].answeredAt;
    assert.ok(gap >= 150 - 25, 'press ' + (i + 1) + ' sent ' + gap + ' ms after the previous END (spacing 150)');
  }
  const rep = w.body.report;
  assert.deepEqual(rep.perBuild.aaa1111.rows['render-basic/t2/jit/8'].seq, table);
  assert.equal(rep.perBuild.aaa1111.rows['render-basic/t2/jit/8'].best, 1.008);
  assert.equal(rep.perBuild.aaa1111.rows['render-basic/t2/jit/8'].median, 0.945);
  assert.equal(rep.device.id, dev.id);
  assert.ok(fs.existsSync(path.join(home, 'jobs', j.id, 'report.md')));
  assert.equal(fs.readdirSync(path.join(home, 'jobs', j.id, 'presses')).length, 3);
  // The legacy result files carry the job keys (D12).
  const legacy = fs.readdirSync(path.join(home, 'results')).filter((f) => f.startsWith('result-'));
  assert.equal(legacy.length, 3);
  const one = JSON.parse(fs.readFileSync(path.join(home, 'results', legacy[0]), 'utf8'));
  assert.equal(one.job, j.id);
  assert.ok(Array.isArray(one.results));
  await dev.stop();
});

test('an A/B 2×3 job runs A B A B A B and stays bound to the first device even when a second joins (D20)', async () => {
  const dev1 = await fakeDevice(lab, { name: 'first', pressMs: 30 });
  let dev2 = null;
  const { body: j } = await queue({ builds: ['aaa1111', 'bbb2222'], rows: 'gate-rows', repeats: 3, spacingMs: 40, ttlMs: 60000 });
  assert.deepEqual(j.presses.map((p) => p.build), ['aaa1111', 'bbb2222', 'aaa1111', 'bbb2222', 'aaa1111', 'bbb2222']);
  // A second device joins after press 1 is out.
  await new Promise((r) => setTimeout(r, 60));
  dev2 = await fakeDevice(lab, { name: 'second', pressMs: 5 });
  const w = await waitJob(j.id);
  assert.equal(w.body.job.state, 'done');
  assert.equal(dev1.answered.length, 6, 'every press went to the first device');
  assert.equal(dev2.answered.length, 0);
  assert.deepEqual(dev1.answered.map((a) => a.build), ['aaa1111', 'bbb2222', 'aaa1111', 'bbb2222', 'aaa1111', 'bbb2222']);
  assert.equal(w.body.job.boundDevice, dev1.id);
  assert.ok(w.body.report.ab['render-basic/t2/jit/8']);
  assert.equal(w.body.report.ab['render-basic/t2/jit/8'].bestDelta, 0);
  await dev1.stop(); await dev2.stop();
});

test('a tainted press is re-queued once (D23): a fourth press appended, the excluded listed, seq has a null', async () => {
  const dev = await fakeDevice(lab, { pressMs: 10, taintOn: new Set([2]) });
  const { body: j } = await queue({ builds: ['aaa1111'], rows: 'gate-rows', repeats: 3, spacingMs: 0, ttlMs: 60000, retryTainted: 1 });
  const w = await waitJob(j.id);
  assert.equal(w.body.job.presses.total, 4);
  assert.equal(w.body.job.presses.tainted, 1);
  assert.deepEqual(w.body.report.excluded, [{ press: 2, build: 'aaa1111', reason: ['hidden'] }]);
  assert.deepEqual(w.body.report.perBuild.aaa1111.rows['render-basic/t2/jit/8'].seq, [1, null, 1, 1]);
  assert.equal(w.body.report.perBuild.aaa1111.rows['render-basic/t2/jit/8'].n, 3);
  await dev.stop();
  // With retries off, the job ends at 3 presses.
  const dev2 = await fakeDevice(lab, { pressMs: 10, taintOn: new Set([1]) });
  const { body: j2 } = await queue({ builds: ['aaa1111'], repeats: 3, spacingMs: 0, ttlMs: 60000, retryTainted: 0 });
  const w2 = await waitJob(j2.id);
  assert.equal(w2.body.job.presses.total, 3);
  await dev2.stop();
});

test('a failed press (worker died) fails the press, the job still completes and says so', async () => {
  const dev = await fakeDevice(lab, { pressMs: 10, failOn: new Set([2]) });
  const { body: j } = await queue({ builds: ['aaa1111'], repeats: 2, spacingMs: 0, ttlMs: 60000 });
  const w = await waitJob(j.id);
  assert.equal(w.body.job.state, 'done');
  assert.equal(w.body.job.presses.failed, 1);
  assert.deepEqual(w.body.report.excluded, [{ press: 2, build: 'aaa1111', reason: ['failed'] }]);
  await dev.stop();
});

test('waits: 408 on timeout with the state; device=any released on join; queue=idle after the last report', async () => {
  const { body: j } = await queue({ builds: ['aaa1111'], repeats: 1, spacingMs: 0, ttlMs: 60000, device: 'nobody-here' });
  const t = await waitJob(j.id, 1);
  assert.equal(t.status, 408);
  assert.equal(t.body.timeout, true);
  assert.equal(t.body.state, 'queued');
  const idle = api('/wait?queue=idle&timeout=1');
  assert.equal((await idle).status, 408, 'a queued job is not idle');
  // device=any: armed before the device exists, released when it joins.
  const anyWait = api('/wait?device=any&timeout=5');
  await new Promise((r) => setTimeout(r, 50));
  const dev = await fakeDevice(lab, { name: 'nobody-here', pressMs: 5 });
  const a = await anyWait;
  assert.equal(a.status, 200);
  assert.equal((await a.json()).device.id, dev.id);
  // The device filter by NAME matches, so the job now runs on it.
  const w = await waitJob(j.id);
  assert.equal(w.body.job.state, 'done');
  assert.equal((await api('/wait?queue=idle&timeout=2')).status, 200);
  await dev.stop();
  // A named wait for a device that is not there times out.
  assert.equal((await api('/wait?device=ghost&timeout=1')).status, 408);
  assert.equal((await api('/wait?job=nope&timeout=1')).status, 404);
  assert.equal((await api('/wait?timeout=1')).status, 400);
});

test('TTL: a job past its TTL never sends and reads expired; a filter for another device never sends', async () => {
  const dev = await fakeDevice(lab, { name: 'me', pressMs: 5 });
  const { body: other } = await queue({ builds: ['aaa1111'], repeats: 1, spacingMs: 0, ttlMs: 60000, device: 'someone-else' });
  await new Promise((r) => setTimeout(r, 100));
  assert.equal(dev.answered.length, 0);
  assert.equal((await (await api('/jobs/' + other.id)).json()).state, 'queued');
  // TTL is validated at ≥ 60 s, so age the file: rewrite createdAt and restart.
  const c = await api('/jobs/' + other.id, { method: 'DELETE' });
  assert.equal((await c.json()).state, 'cancelled');
  await dev.stop();
  await lab.stop();
  const { body: stale } = await (async () => {
    lab = await startServer(home, FAST);
    return queue({ builds: ['aaa1111'], repeats: 1, spacingMs: 0, ttlMs: 60000 });
  })();
  await lab.stop();
  const f = path.join(home, 'jobs', stale.id + '.json');
  const jf = JSON.parse(fs.readFileSync(f, 'utf8'));
  jf.createdAt = new Date(Date.now() - 120000).toISOString();
  fs.writeFileSync(f, JSON.stringify(jf));
  lab = await startServer(home, FAST);
  const dev2 = await fakeDevice(lab, { pressMs: 5 });
  const w = await waitJob(stale.id, 5);
  assert.equal(w.status, 200);
  assert.equal(w.body.job.state, 'expired');
  assert.equal(dev2.answered.length, 0);
  await dev2.stop();
});

test('restart: a press that was sent when the server died is re-sent once and the job completes', async () => {
  let killed = false;
  const dev = await fakeDevice(lab, {
    pressMs: 5,
    // Answer nothing the first time press 2 arrives: the server is killed
    // under it instead.
    beforeAnswer: async (p) => { if (p.press === 2 && !killed) { killed = true; await lab.stop(); throw new Error('server killed under press 2'); } },
  });
  const { body: j } = await queue({ builds: ['aaa1111'], repeats: 3, spacingMs: 0, ttlMs: 60000 });
  await new Promise((r) => setTimeout(r, 400));
  assert.equal(killed, true);
  const onDisk = JSON.parse(fs.readFileSync(path.join(home, 'jobs', j.id + '.json'), 'utf8'));
  assert.equal(onDisk.presses[1].state, 'sent', 'press 2 was in flight when the server died');
  await dev.stop().catch(() => {});
  lab = await startServer(home, FAST);
  const loadedResp = await api('/jobs/' + j.id);
  assert.equal(loadedResp.status, 200, 'job reloaded from disk: ' + lab.stderr());
  const loaded = await loadedResp.json();
  assert.equal(loaded.presses[1].state, 'pending');
  assert.equal(loaded.presses[1].resends, 1);
  // The same device comes back (a phone reloads; its id is in localStorage):
  // the job is bound to it (D20), so another id would not be offered press 2.
  const dev2 = await fakeDevice(lab, { id: dev.id, pressMs: 5 });
  const w = await waitJob(j.id);
  assert.equal(w.status, 200, JSON.stringify(w.body));
  assert.equal(w.body.job.state, 'done');
  assert.deepEqual(dev2.answered.map((a) => a.press), [2, 3]);
  await dev2.stop();
});

test('lost: a sent press with no result is re-sent once, then failed', async () => {
  let seen = 0;
  const dev = await fakeDevice(lab, { pressMs: 5, beforeAnswer: async () => { seen++; throw new Error('never answers'); } });
  const { body: j } = await queue({ builds: ['aaa1111'], repeats: 1, spacingMs: 0, ttlMs: 60000 });
  const w = await waitJob(j.id, 5);
  assert.equal(w.status, 200);
  assert.equal(w.body.job.state, 'done');
  assert.equal(w.body.job.presses.failed, 1);
  assert.equal(seen, 2, 'sent twice, never a third time');
  await dev.stop().catch(() => {});
});

// B: the observed failure was a press sent to a phone whose tab went away
// seconds later. Nothing but the wall bound (wallTimeout × rows + 120 s =
// 3120 s on that job) could retire it, so the phone came back and sat idle
// for 40 minutes. A page with no stream cannot post a result, so a closed
// stream past the drop window is the press's answer.
test('drop: a press whose device goes away before any result is lost at the drop window, not the wall bound (B)', async () => {
  // A wall bound a minute out, a drop window a quarter-second out: inside
  // this test only the drop rule can fire.
  const l = await ownLab({ LAB_TICK_MS: '20', LAB_COOLDOWN_MS: '0', LAB_LOST_MS: '60000', LAB_DROP_LOST_MS: '250' });
  try {
    let seen = 0;
    const dev = await fakeDevice(l, { pressMs: 5, beforeAnswer: async () => { seen++; throw new Error('the tab went away'); } });
    const { body: j } = await l.queue({ builds: ['aaa1111'], repeats: 1, spacingMs: 0, ttlMs: 60000 });
    await until('press 1 to reach the device', async () => seen === 1);
    const sent = await l.job(j.id);
    assert.equal(sent.presses[0].state, 'sent');
    assert.equal(sent.boundDevice, dev.id);
    await dev.stop().catch(() => {});
    const back = await until('press 1 back to pending', async () => {
      const cur = await l.job(j.id);
      return cur.presses[0].state === 'pending' ? cur : null;
    }, 5000);
    assert.equal(back.presses[0].resends, 1, 'lost once, one re-send owed');
    const logged = fs.readFileSync(path.join(l.home, 'log', 'server.log'), 'utf8');
    assert.match(logged, /press 1 lost \(device stream closed .* no result\)/, 'the drop rule named it, not the wall bound');
    // The phone comes back with the id it kept in localStorage: the job is
    // bound to it (D20), and the re-send is waiting.
    const dev2 = await fakeDevice(l, { id: dev.id, pressMs: 5 });
    const w = await apiOn(l, '/wait?job=' + j.id + '&timeout=10');
    assert.equal(w.status, 200);
    const wb = await w.json();
    assert.equal(wb.job.state, 'done');
    assert.deepEqual(dev2.answered.map((a) => a.press), [1], 'the re-send ran on the reconnected device');
    await dev2.stop();
  } finally { await l.stop(); }
});

test('drop: a stream flicker shorter than the window leaves the press sent (B)', async () => {
  const l = await ownLab({ LAB_TICK_MS: '20', LAB_COOLDOWN_MS: '0', LAB_LOST_MS: '60000', LAB_DROP_LOST_MS: '1500' });
  try {
    let seen = 0;
    const quiet = async () => { seen++; throw new Error('running it, no result yet'); };
    const dev = await fakeDevice(l, { pressMs: 5, beforeAnswer: quiet });
    const { body: j } = await l.queue({ builds: ['aaa1111'], repeats: 1, spacingMs: 0, ttlMs: 60000 });
    await until('press 1 to reach the device', async () => seen === 1);
    await dev.stop().catch(() => {});
    // Back well inside the window, the way an SSE stream reconnects.
    const again = await fakeDevice(l, { id: dev.id, pressMs: 5, beforeAnswer: quiet });
    // Now past the window measured from the FIRST close: only a cleared drop
    // clock keeps the press alive here.
    await sleep(2200);
    const cur = await l.job(j.id);
    assert.equal(cur.presses[0].state, 'sent', 'a flicker is not a device that went away');
    assert.equal(cur.presses[0].resends, 0);
    assert.equal(seen, 1, 'never re-sent');
    await again.stop();
  } finally { await l.stop(); }
});

test('drop: a stream that stays open past the wall bound still hits the wall rule (B)', async () => {
  const l = await ownLab({ LAB_TICK_MS: '20', LAB_COOLDOWN_MS: '0', LAB_LOST_MS: '400', LAB_DROP_LOST_MS: '60000' });
  try {
    let seen = 0;
    const dev = await fakeDevice(l, { pressMs: 5, beforeAnswer: async () => { seen++; throw new Error('never answers'); } });
    const { body: j } = await l.queue({ builds: ['aaa1111'], repeats: 1, spacingMs: 0, ttlMs: 60000 });
    const r = await apiOn(l, '/wait?job=' + j.id + '&timeout=10');
    assert.equal(r.status, 200);
    const wb = await r.json();
    assert.equal(wb.job.state, 'done');
    assert.equal(wb.job.presses.failed, 1);
    assert.equal(seen, 2, 'sent twice on the wall bound, never a third time');
    const logged = fs.readFileSync(path.join(l.home, 'log', 'server.log'), 'utf8');
    assert.match(logged, /press 1 lost \(no result in /, 'the wall bound named it');
    assert.doesNotMatch(logged, /device stream closed .* no result/, 'the stream never closed, so the drop rule never fired');
    await dev.stop().catch(() => {});
  } finally { await l.stop(); }
});

// C: the cooldown is sized by the burn (DD5/DD6). Every test below pins the
// floor and the ceiling apart from the press it uses, so the number the
// scheduler actually waited names which of the three rules fired.
const COOLDOWN_ENV = { LAB_TICK_MS: '20', LAB_LOST_MS: '60000' };
/// The gap between the END of press n and the SEND of press n+1 — the wait the
/// cooldown bought, measured the way the first test in this file measures
/// spacing.
const gapAfter = (dev, n) => dev.answered[n].sentAt - dev.answered[n - 1].answeredAt;

test('cooldown: a press is followed by ~1× its own duration, not the ceiling and not the floor (C)', async () => {
  // A 100 ms floor and a 4 s ceiling around a ~800 ms press: only the
  // proportional rule can land in between.
  const l = await ownLab({ ...COOLDOWN_ENV, LAB_COOLDOWN_MS: '4000', LAB_COOLDOWN_FLOOR_MS: '100' });
  try {
    const dev = await fakeDevice(l, { pressMs: 800 });
    await l.queue({ builds: ['aaa1111'], repeats: 2, spacingMs: 0, ttlMs: 60000 });
    await until('both presses answered', async () => dev.answered.length === 2, 20000);
    const gap = gapAfter(dev, 1);
    assert.ok(gap >= 700, 'press 2 waited about the 800 ms burn, not the 100 ms floor (waited ' + gap + ' ms)');
    assert.ok(gap <= 2000, 'and nowhere near the 4 s ceiling (waited ' + gap + ' ms)');
    const st = await apiOn(l, '/status').then((r) => r.json());
    assert.equal(st.config.cooldownFactor, 1);
    assert.equal(st.config.cooldownFloorMs, 100);
    assert.equal(st.config.cooldownMs, 4000, 'cooldownMs is the ceiling');
    const row = st.devices.find((x) => x.id === dev.id);
    assert.ok(row.lastPressDurationMs >= 700 && row.lastPressDurationMs <= 2000, 'the burn is on the device record (' + row.lastPressDurationMs + ' ms)');
    assert.equal(row.cooldownMs, Math.round(row.lastPressDurationMs), 'factor 1: the cooldown IS the burn');
    await dev.stop();
  } finally { await l.stop(); }
});

test('cooldown: a press shorter than the floor waits the floor (C)', async () => {
  const l = await ownLab({ ...COOLDOWN_ENV, LAB_COOLDOWN_MS: '4000', LAB_COOLDOWN_FLOOR_MS: '600' });
  try {
    const dev = await fakeDevice(l, { pressMs: 5 });
    await l.queue({ builds: ['aaa1111'], repeats: 2, spacingMs: 0, ttlMs: 60000 });
    await until('both presses answered', async () => dev.answered.length === 2, 20000);
    const gap = gapAfter(dev, 1);
    assert.ok(gap >= 520, 'a 5 ms press still rests the 600 ms floor (waited ' + gap + ' ms)');
    assert.ok(gap <= 1600, 'the floor, not the ceiling (waited ' + gap + ' ms)');
    const st = await apiOn(l, '/status').then((r) => r.json());
    assert.equal(st.devices.find((x) => x.id === dev.id).cooldownMs, 600);
    await dev.stop();
  } finally { await l.stop(); }
});

test('cooldown: a press longer than the ceiling waits the ceiling, not more (C)', async () => {
  const l = await ownLab({ ...COOLDOWN_ENV, LAB_COOLDOWN_MS: '300', LAB_COOLDOWN_FLOOR_MS: '50' });
  try {
    const dev = await fakeDevice(l, { pressMs: 900 });
    await l.queue({ builds: ['aaa1111'], repeats: 2, spacingMs: 0, ttlMs: 60000 });
    await until('both presses answered', async () => dev.answered.length === 2, 20000);
    const gap = gapAfter(dev, 1);
    assert.ok(gap >= 250, 'the ceiling is still a wait (waited ' + gap + ' ms)');
    assert.ok(gap <= 700, 'clamped to the 300 ms ceiling, not the ~900 ms burn (waited ' + gap + ' ms)');
    const st = await apiOn(l, '/status').then((r) => r.json());
    assert.equal(st.devices.find((x) => x.id === dev.id).cooldownMs, 300);
    await dev.stop();
  } finally { await l.stop(); }
});

// The whole FAST suite above runs on `LAB_COOLDOWN_MS: '0'` and would not
// finish in its timeouts if 0 had stopped meaning OFF, but that is an
// implication, so here it is said out loud.
test('cooldown: LAB_COOLDOWN_MS=0 is still OFF — presses go back to back (C)', async () => {
  const dev = await fakeDevice(lab, { name: 'no-cooldown', pressMs: 5 });
  const { body: j } = await queue({ builds: ['aaa1111'], repeats: 3, spacingMs: 0, ttlMs: 60000, device: 'no-cooldown' });
  const w = await waitJob(j.id);
  assert.equal(w.body.job.state, 'done');
  for (let i = 1; i < dev.answered.length; i++) {
    assert.ok(gapAfter(dev, i) <= 400, 'press ' + (i + 1) + ' followed press ' + i + ' at once (' + gapAfter(dev, i) + ' ms)');
  }
  const st = await api('/status').then((r) => r.json());
  assert.equal(st.config.cooldownMs, 0, '/status reports the effective ceiling, which is off');
  assert.equal(st.devices.find((x) => x.id === dev.id).cooldownMs, 0);
  await dev.stop();
});

test('cooldown: the burn survives a restart, so the next press still waits 1× it, not the ceiling (C)', async () => {
  const env = { ...COOLDOWN_ENV, LAB_COOLDOWN_MS: '4000', LAB_COOLDOWN_FLOOR_MS: '100' };
  const l = await ownLab(env);
  let l2 = null;
  try {
    const dev = await fakeDevice(l, { pressMs: 800 });
    await l.queue({ builds: ['aaa1111'], repeats: 1, spacingMs: 0, ttlMs: 60000 });
    await until('the press to land', async () => dev.answered.length === 1, 20000);
    await dev.stop();
    const onDisk = JSON.parse(fs.readFileSync(path.join(l.home, 'devices', dev.id + '.json'), 'utf8'));
    assert.ok(onDisk.lastPressDurationMs >= 700, 'the burn is on the device FILE (' + onDisk.lastPressDurationMs + ' ms)');
    await l.stop();
    l2 = await startServer(l.home, env);
    const st = await apiOn(l2, '/status').then((r) => r.json());
    const row = st.devices.find((x) => x.id === dev.id);
    assert.equal(row.lastPressDurationMs, onDisk.lastPressDurationMs, 'the restarted server remembers it');
    assert.equal(row.cooldownMs, Math.round(onDisk.lastPressDurationMs), 'and still owes 1× it, not the 4 s ceiling');
  } finally { if (l2) await l2.stop(); await l.stop().catch(() => {}); }
});

// A device the lab has never watched finish a press — a record written before
// this model, or one whose press was lost — has no burn to size a cooldown
// from, so it waits the ceiling.
test('cooldown: a device with no known burn waits the ceiling (C)', async () => {
  const l = await ownLab({ ...COOLDOWN_ENV, LAB_COOLDOWN_MS: '4000', LAB_COOLDOWN_FLOOR_MS: '100' });
  try {
    const dev = await fakeDevice(l, { pressMs: 5 });
    // The shape a pre-C server left behind: an end time, no duration.
    const f = path.join(l.home, 'devices', dev.id + '.json');
    const d = JSON.parse(fs.readFileSync(f, 'utf8'));
    delete d.lastPressDurationMs;
    d.lastPressEndAt = new Date().toISOString();
    fs.writeFileSync(f, JSON.stringify(d));
    const st = await apiOn(l, '/status').then((r) => r.json());
    assert.equal(st.devices.find((x) => x.id === dev.id).cooldownMs, 4000, 'unknown burn falls back to the ceiling');
    await dev.stop();
  } finally { await l.stop(); }
});

test('a device that reconnects gets the queue view again even though nothing changed', async () => {
  // A device that never answers, so the job stays in its view across the reload.
  const quiet = { beforeAnswer: async () => { throw new Error('never answers'); } };
  const dev = await fakeDevice(lab, { name: 'reloader', pressMs: 5, ...quiet });
  const { body: j } = await queue({ builds: ['aaa1111'], repeats: 1, spacingMs: 0, ttlMs: 60000, device: 'reloader' });
  // The stream's first queue event is the one sent at open, before this
  // job existed; the job's own event follows.
  let first;
  for (let i = 0; i < 5; i++) { first = await dev.sse.next('queue', 3000).catch((e) => { throw new Error('phase 1, event ' + i + ': ' + e.message + ' (seen: ' + dev.sse.events().join(',') + ')'); }); if (first.jobs.some((x) => x.id === j.id)) break; }
  assert.ok(first.jobs.some((x) => x.id === j.id), 'the view lists the job');
  assert.ok(Array.isArray(first.jobs.find((x) => x.id === j.id).pressStates), 'per-press states ride along');
  await dev.stop();
  const again = await fakeDevice(lab, { id: dev.id, name: 'reloader', pressMs: 5, ...quiet });
  const second = await again.sse.next('queue', 3000).catch((e) => { throw new Error('phase 2: ' + e.message + ' (seen: ' + again.sse.events().join(',') + ')'); });
  assert.ok(second.jobs.some((x) => x.id === j.id), 'the reloaded page is told about the job again');
  await again.stop();
  await api('/jobs/' + j.id, { method: 'DELETE' });
});

test('POST /jobs: 401 without the token creates no file; unknown build, bad rows, bad repeats are 400', async () => {
  const before = fs.readdirSync(path.join(home, 'jobs')).length;
  const r = await fetch(lab.url + '/jobs', { method: 'POST', body: JSON.stringify({ builds: ['aaa1111'] }) });
  assert.equal(r.status, 401);
  assert.equal(fs.readdirSync(path.join(home, 'jobs')).length, before);
  assert.equal((await queue({ builds: ['nope'] })).status, 400);
  assert.equal((await queue({ builds: ['aaa1111'], rows: [] })).status, 400);
  assert.equal((await queue({ builds: ['aaa1111'], rows: [{ slug: 'ghost', grade: 't2', mode: 'jit' }] })).status, 400);
  assert.equal((await queue({ builds: ['aaa1111'], repeats: 0 })).status, 400);
  assert.equal((await queue({ builds: ['aaa1111'], kind: 'walk' })).status, 400);
  assert.equal((await queue({ builds: ['aaa1111', 'aaa1111'] })).status, 400);
  assert.equal((await queue({ builds: ['aaa1111'], ttlMs: 1000 })).status, 400);
  const ok = await queue({ builds: ['aaa1111'], rows: [{ slug: 'render-basic', grade: 't2', mode: 'jit', fnBlocks: 8 }, { slug: 'render-basic', grade: 't2', mode: 'interp' }], repeats: 1, ttlMs: 60000, device: 'nobody' });
  assert.equal(ok.status, 201);
  assert.equal(ok.body.rows[1].fnBlocks, null);
  assert.equal(ok.body.rows[0].fnBlocks, 8);
  await api('/jobs/' + ok.body.id, { method: 'DELETE' });
});

// C: the coded default is 0 — cooldown-governed. The cooldown is the thermal
// rule and it is sized by each press's own burn, so a flat spacing default
// could only stretch a job past what the device asked for. An explicit
// spacing is still for a job you want deliberately slower than that.
test('POST /jobs: spacingMs defaults to 0 (cooldown-governed); an explicit spacing is still honoured', async () => {
  const dflt = await queue({ builds: ['aaa1111'], repeats: 1, ttlMs: 60000, device: 'nobody' });
  assert.equal(dflt.status, 201);
  assert.equal(dflt.body.spacingMs, 0);
  const slow = await queue({ builds: ['aaa1111'], repeats: 1, ttlMs: 60000, device: 'nobody', spacingMs: 180000 });
  assert.equal(slow.status, 201);
  assert.equal(slow.body.spacingMs, 180000);
  await api('/jobs/' + dflt.body.id, { method: 'DELETE' });
  await api('/jobs/' + slow.body.id, { method: 'DELETE' });
});

// D: a job says who queued it. `lab.sh` supplies the default (its own flag,
// $LAB_BY, or $USER@host); the server never invents one, so a job posted with
// no `by` is honestly anonymous.
test('POST /jobs: `by` round-trips to the job view and the job file; absent is null; too long is 400', async () => {
  const mine = await queue({ builds: ['aaa1111'], repeats: 1, ttlMs: 60000, device: 'nobody', by: '  emu-lab-polish-d1a04a-bb  ' });
  assert.equal(mine.status, 201);
  assert.equal(mine.body.by, 'emu-lab-polish-d1a04a-bb', 'the create response carries it, trimmed');
  const file = JSON.parse(fs.readFileSync(path.join(home, 'jobs', mine.body.id + '.json'), 'utf8'));
  assert.equal(file.by, 'emu-lab-polish-d1a04a-bb', 'and so does the job file');
  const view = await api('/jobs/' + mine.body.id).then((r) => r.json());
  assert.equal(view.by, 'emu-lab-polish-d1a04a-bb', 'and the job view');
  const listed = await api('/jobs').then((r) => r.json());
  assert.equal(listed.jobs.find((j) => j.id === mine.body.id).by, 'emu-lab-polish-d1a04a-bb');

  const anon = await queue({ builds: ['aaa1111'], repeats: 1, ttlMs: 60000, device: 'nobody' });
  assert.equal(anon.status, 201);
  assert.equal(anon.body.by, null, 'the server invents nobody');
  const blank = await queue({ builds: ['aaa1111'], repeats: 1, ttlMs: 60000, device: 'nobody', by: '   ' });
  assert.equal(blank.body.by, null, 'blank is the same as absent');

  assert.equal((await queue({ builds: ['aaa1111'], ttlMs: 60000, device: 'nobody', by: 'x'.repeat(65) })).status, 400);
  assert.equal((await queue({ builds: ['aaa1111'], ttlMs: 60000, device: 'nobody', by: 42 })).status, 400);
  const ok64 = await queue({ builds: ['aaa1111'], ttlMs: 60000, device: 'nobody', by: 'x'.repeat(64) });
  assert.equal(ok64.status, 201, '64 is the limit, not one under it');

  for (const id of [mine.body.id, anon.body.id, blank.body.id, ok64.body.id]) await api('/jobs/' + id, { method: 'DELETE' });
});

// The page's job card reads the queue view, so `by` has to ride that event and
// not only the director's job views.
test('the queue view the page receives carries `by`', async () => {
  const quiet = { beforeAnswer: async () => { throw new Error('never answers'); } };
  const dev = await fakeDevice(lab, { name: 'card-reader', pressMs: 5, ...quiet });
  const { body: j } = await queue({ builds: ['aaa1111'], repeats: 1, spacingMs: 0, ttlMs: 60000, device: 'card-reader', by: 'emu-lab-job-by-xx' });
  let seen;
  for (let i = 0; i < 5; i++) { const ev = await dev.sse.next('queue', 3000); seen = ev.jobs.find((x) => x.id === j.id); if (seen) break; }
  assert.ok(seen, 'the view lists the job');
  assert.equal(seen.by, 'emu-lab-job-by-xx');
  await dev.stop();
  await api('/jobs/' + j.id, { method: 'DELETE' });
});
