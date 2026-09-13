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
const api = (p, opts = {}) => fetch(lab.url + p, { ...opts, headers: { Authorization: 'Bearer ' + lab.token, 'Content-Type': 'application/json', ...(opts.headers || {}) } });
const queue = async (body) => { const r = await api('/jobs', { method: 'POST', body: JSON.stringify(body) }); return { status: r.status, body: await r.json() }; };
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
