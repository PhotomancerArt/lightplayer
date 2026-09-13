// P1: the lab server core — static files, token, presence, results, status.
//
// Spawns the real server as a child on an OS-assigned port with a temp home,
// so what is tested is the process launchd will run, not a harness of it.
'use strict';

import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { startServer, readSse } from './helpers.mjs';

let lab;

before(async () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'emu-lab-test-'));
  // A staged-build fixture: a manifest, a worker, and an ELF that is a symlink
  // into images/ (D16) — plus a planted symlink that points out of the home.
  fs.mkdirSync(path.join(home, 'builds', 'abc1234'), { recursive: true });
  fs.mkdirSync(path.join(home, 'images'), { recursive: true });
  fs.writeFileSync(path.join(home, 'images', 'deadbeefcafe.elf'), Buffer.from([0x7f, 0x45, 0x4c, 0x46, 1, 2, 3]));
  fs.symlinkSync('../../images/deadbeefcafe.elf', path.join(home, 'builds', 'abc1234', 'fw-x.elf'));
  fs.writeFileSync(path.join(home, 'builds', 'abc1234', 'worker.js'), '// worker\n');
  fs.writeFileSync(path.join(home, 'builds', 'abc1234', 'manifest.json'), JSON.stringify({
    build: { id: 'abc1234', short: 'abc1234', branch: 'x', dirty: false, built_at: '2026-09-11T00:00:00Z', wasm_bytes: 1 },
    images: [{ slug: 'x', elf: 'fw-x.elf', elfStamp: 'deadbeefcafe' }],
  }));
  fs.symlinkSync('/etc/hosts', path.join(home, 'builds', 'evil'));
  lab = await startServer(home);
});

after(() => lab && lab.stop());

test('the page is served no-store; escapes and planted symlinks are 404', async () => {
  const r = await fetch(lab.url + '/');
  assert.equal(r.status, 200);
  assert.equal(r.headers.get('cache-control'), 'no-store');
  assert.match(await r.text(), /<html/i);
  // A `..` is normalised away by the URL parser before routing, so it lands
  // on a token-guarded path (401); a planted symlink, a dot-dir or an escape
  // that survives parsing is a 404. Either way: nothing is served.
  for (const p of ['/builds/../token', '/builds/x/../../token', '/builds/evil', '/images/../token', '/builds/.tmp-x/manifest.json', '/builds/%2e%2e/token']) {
    const e = await fetch(lab.url + p);
    assert.ok(e.status === 404 || e.status === 401, p + ' -> ' + e.status);
    assert.doesNotMatch(await e.text(), /^[0-9a-f]{32}$/m, p + ' leaked the token');
  }
});

test('/status needs the token; with it lists the fixture build and no devices', async () => {
  assert.equal((await fetch(lab.url + '/status')).status, 401);
  assert.equal((await fetch(lab.url + '/status?t=' + 'f'.repeat(32))).status, 401);
  const r = await fetch(lab.url + '/status', { headers: { Authorization: 'Bearer ' + lab.token } });
  assert.equal(r.status, 200);
  const s = await r.json();
  assert.deepEqual(s.devices, []);
  assert.equal(s.builds.length, 1);
  assert.equal(s.builds[0].id, 'abc1234');
  assert.deepEqual(s.jobs, { queued: 0, running: 0, done: 0 });
  assert.equal(s.results, 0);
  // The query form is what EventSource uses.
  assert.equal((await fetch(lab.url + '/status?t=' + lab.token)).status, 200);
  assert.equal((await fetch(lab.url + '/healthz')).status, 200);
});

test('presence: stream open + visible = present; hidden or closed = not', async () => {
  const ac = new AbortController();
  const sse = await readSse(lab.url + '/events?device=d1&t=' + lab.token, ac.signal);
  const hello = await sse.next('hello');
  assert.equal(hello.device.id, 'd1');
  assert.equal(typeof hello.config.cooldownMs, 'number');

  const status = async () => (await (await fetch(lab.url + '/status?t=' + lab.token)).json()).devices.find((d) => d.id === 'd1');
  assert.equal((await status()).present, false, 'no state yet');

  const post = (state) => fetch(lab.url + '/devices/d1/state', {
    method: 'POST', headers: { Authorization: 'Bearer ' + lab.token, 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: 'phone', ua: 'test-ua', cores: 4, deviceMemory: null, ...state }),
  });
  assert.equal((await post({ visibility: 'visible', hasFocus: true, wakeLock: 'active' })).status, 200);
  let d = await status();
  assert.equal(d.present, true);
  assert.equal(d.name, 'phone');
  assert.equal(d.lastState.wakeLock, 'active');

  await post({ visibility: 'hidden', hasFocus: false, wakeLock: 'released' });
  assert.equal((await status()).present, false, 'hidden is not present');
  await post({ visibility: 'visible', hasFocus: true, wakeLock: 'active' });
  assert.equal((await status()).present, true);

  const seenBefore = (await status()).lastSeen;
  await new Promise((r) => setTimeout(r, 5));
  ac.abort();
  await sse.closed;
  await new Promise((r) => setTimeout(r, 50));
  d = await status();
  assert.equal(d.present, false, 'closed stream is not present');
  assert.equal(d.streams, 0);
  assert.notEqual(d.lastSeen, seenBefore, 'lastSeen updated on close');
  assert.ok(fs.existsSync(path.join(lab.home, 'devices', 'd1.json')));

  assert.equal((await fetch(lab.url + '/events?device=d1')).status, 401);
});

test('manual results: named by the server, legacy keys kept, capped, validated, never colliding', async () => {
  const legacy = { ua: 'test-ua', cores: 4, at: '2026-09-11T00:00:00.000Z', build: { short: 'abc1234' }, deviceMemory: null, preset: 'gate-rows', results: [{ slug: 'x', grade: 't2', mode: 'jit', fnBlocks: 8, realtime: 1.0 }] };
  const post = (body, q = '?device=d1') => fetch(lab.url + '/results/manual' + q, {
    method: 'POST', headers: { Authorization: 'Bearer ' + lab.token, 'Content-Type': 'application/json' }, body,
  });
  const [a, b] = await Promise.all([post(JSON.stringify(legacy)), post(JSON.stringify(legacy))]);
  assert.equal(a.status, 200);
  assert.equal(b.status, 200);
  const fa = (await a.json()).file, fb = (await b.json()).file;
  assert.notEqual(fa, fb, 'two posts never share a file');
  const written = JSON.parse(fs.readFileSync(path.join(lab.home, fa), 'utf8'));
  for (const k of Object.keys(legacy)) assert.deepEqual(written[k], legacy[k], 'legacy key ' + k);
  assert.equal(written.manual, true);
  assert.equal(written.device, 'd1');
  assert.ok(written.receivedAt);

  assert.equal((await post(JSON.stringify({ ...legacy, results: 'nope' }))).status, 400);
  assert.equal((await post('not json')).status, 400);
  const big = JSON.stringify({ ...legacy, pad: 'x'.repeat(3 * 1024 * 1024) });
  assert.equal((await post(big)).status, 413);
  assert.equal((await fetch(lab.url + '/results/manual', { method: 'POST', body: '{}' })).status, 401);
  const files = fs.readdirSync(path.join(lab.home, 'results'));
  assert.equal(files.length, 2, 'refused posts wrote nothing');
});

test('a staged build is served: immutable on ?v=, no-store on the manifest, ELF bytes through the symlink', async () => {
  const w = await fetch(lab.url + '/builds/abc1234/worker.js?v=abc1234');
  assert.equal(w.status, 200);
  assert.equal(w.headers.get('cache-control'), 'public, max-age=31536000, immutable');
  assert.equal(w.headers.get('content-type'), 'text/javascript; charset=utf-8');
  const m = await fetch(lab.url + '/builds/abc1234/manifest.json?t=1');
  assert.equal(m.headers.get('cache-control'), 'no-store');
  const e = await fetch(lab.url + '/builds/abc1234/fw-x.elf?v=deadbeefcafe');
  assert.equal(e.status, 200);
  assert.equal(e.headers.get('content-length'), '7');
  assert.equal(e.headers.get('cache-control'), 'public, max-age=31536000, immutable');
  const bytes = new Uint8Array(await e.arrayBuffer());
  assert.deepEqual(Array.from(bytes.slice(0, 4)), [0x7f, 0x45, 0x4c, 0x46]);
  const unstamped = await fetch(lab.url + '/images/deadbeefcafe.elf');
  assert.equal(unstamped.headers.get('cache-control'), 'no-cache');
});
