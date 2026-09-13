// F2: one notification when jobs wait with no device present.
//
// The acceptance numbers, at test speed: a job queued with no device produces
// exactly ONE notification; a device that is present produces none; a burst of
// three queued jobs produces one. Nothing here reaches the network —
// `LAB_NOTIFY_CMD` is the channel for every test in this file, and it appends
// a line to a file in the temp home, so "how many notifications" is "how many
// lines".
'use strict';

import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { startServer } from './helpers.mjs';
import { fakeDevice } from './fake-device.mjs';
import { readNotifyConfig } from '../notify.mjs';

const FAST = { LAB_TICK_MS: '20', LAB_COOLDOWN_MS: '0', LAB_LOST_MS: '100000' };
const TICKS = (n = 12) => new Promise((r) => setTimeout(r, 20 * n));

function tempHome(notify) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'emu-lab-notify-'));
  const config = { port: 0, cooldownMs: 0, maxResultBytes: 2000000 };
  if (notify !== undefined) config.notify = notify;
  fs.writeFileSync(path.join(home, 'config.json'), JSON.stringify(config, null, 2) + '\n');
  fs.mkdirSync(path.join(home, 'builds', 'aaa1111'), { recursive: true });
  fs.writeFileSync(path.join(home, 'builds', 'aaa1111', 'manifest.json'), JSON.stringify({
    build: { id: 'aaa1111', short: 'aaa1111', branch: 'x', dirty: false, built_at: '2026-09-11T00:00:00Z' },
    images: [{ slug: 'render-basic', elf: 'fw-render-basic.elf' }],
    defaults: { wallTimeout: 600, exitOn: false, fnBlocks: 32, timeout: '5500ms' },
  }, null, 2) + '\n');
  return home;
}

/// The stub sender: one line per notification, title and body, in the home.
function callsFile(home) { return path.join(home, 'notify-calls.log'); }
function stubEnv(home) {
  return { LAB_NOTIFY_CMD: 'printf "%s|%s\\n" "$LAB_NOTIFY_TITLE" "$LAB_NOTIFY_BODY" >> "' + callsFile(home) + '"' };
}
function calls(home) {
  try { return fs.readFileSync(callsFile(home), 'utf8').trim().split('\n').filter(Boolean); } catch { return []; }
}

// A real grace of 30 s, shrunk to 150 ms: the burst below lands inside it,
// which is exactly what the grace is for.
const NTFY = { kind: 'ntfy', topic: 'lab-test-topic', graceMs: 150, minIntervalMs: 3600000 };

function api(lab, p, opts = {}) {
  return fetch(lab.url + p, { ...opts, headers: { Authorization: 'Bearer ' + lab.token, 'Content-Type': 'application/json', ...(opts.headers || {}) } });
}
const queue = (lab, body = {}) => api(lab, '/jobs', { method: 'POST', body: JSON.stringify({ builds: ['aaa1111'], rows: 'gate-rows', repeats: 1, spacingMs: 0, ttlMs: 600000, ...body }) }).then((r) => r.json());

test('a job queued with no device present sends exactly one notification, and says what is waiting', async () => {
  const home = tempHome(NTFY);
  const lab = await startServer(home, { ...FAST, ...stubEnv(home) });
  try {
    const j = await queue(lab, { note: 'F2' });
    await TICKS(15);
    const lines = calls(home);
    assert.equal(lines.length, 1, 'expected one notification, got ' + JSON.stringify(lines));
    const [title, body] = lines[0].split('|');
    assert.equal(title, 'emu-lab: 1 job waiting');
    assert.match(body, /No device present\. 1 press pending: /);
    assert.ok(body.includes(j.id), 'the body names the job: ' + body);
    // Disarmed and recorded, so a restart does not repeat it.
    const st = JSON.parse(fs.readFileSync(path.join(home, 'notify.json'), 'utf8'));
    assert.equal(st.armed, false);
    assert.ok(st.notifiedAt, 'notifiedAt recorded');
    assert.match(st.lastResult, /^queue waiting, no device: ntfy 0/);
    // It keeps being true, tick after tick, and stays one notification.
    await TICKS(20);
    assert.equal(calls(home).length, 1);
  } finally { await lab.stop(); }
});

test('a device present takes the presses and nothing is notified', async () => {
  const home = tempHome(NTFY);
  const lab = await startServer(home, { ...FAST, ...stubEnv(home) });
  const dev = await fakeDevice(lab, { pressMs: 10 });
  try {
    const j = await queue(lab, { repeats: 3 });
    const w = await api(lab, '/wait?job=' + j.id + '&timeout=10').then((r) => r.json());
    assert.equal(w.job.state, 'done');
    await TICKS(15);
    assert.deepEqual(calls(home), [], 'a present device must produce no notification');
    // It never even had a state to write: armed, nothing waiting on nobody.
    assert.ok(!fs.existsSync(path.join(home, 'notify.json')), 'no notify state was written');
    const s = await api(lab, '/status').then((r) => r.json());
    assert.equal(s.notify.configured, true);
    assert.equal(s.notify.armed, true);
    assert.equal(s.notify.notifiedAt, null);
    assert.equal(s.notify.kind, 'ntfy');
    assert.ok(!JSON.stringify(s.notify).includes('lab-test-topic'), '/status must not echo the topic');
  } finally { await dev.stop(); await lab.stop(); }
});

test('three jobs queued in a burst with no device are one notification, naming all three', async () => {
  const home = tempHome(NTFY);
  const lab = await startServer(home, { ...FAST, ...stubEnv(home) });
  try {
    const a = await queue(lab, { note: 'one' });
    const b = await queue(lab, { note: 'two' });
    const c = await queue(lab, { note: 'three' });
    await TICKS(25);
    const lines = calls(home);
    assert.equal(lines.length, 1, 'a burst is one episode, got ' + JSON.stringify(lines));
    assert.equal(lines[0].split('|')[0], 'emu-lab: 3 jobs waiting');
    for (const j of [a, b, c]) assert.ok(lines[0].includes(j.id), 'names ' + j.id);
  } finally { await lab.stop(); }
});

test('a restart on the same home does not re-send what it already sent', async () => {
  const home = tempHome(NTFY);
  let lab = await startServer(home, { ...FAST, ...stubEnv(home) });
  try {
    await queue(lab);
    await TICKS(15);
    assert.equal(calls(home).length, 1);
    await lab.stop();
    lab = await startServer(home, { ...FAST, ...stubEnv(home) });
    await TICKS(25);
    assert.equal(calls(home).length, 1, 'the state file survived the restart');
  } finally { await lab.stop(); }
});

test('a device arriving re-arms it: the next absence with work waiting notifies again', async () => {
  // The floor is off here so the second send is the arm's doing, not a timer's.
  const home = tempHome({ ...NTFY, minIntervalMs: 0 });
  const lab = await startServer(home, { ...FAST, ...stubEnv(home) });
  try {
    await queue(lab, { repeats: 4, spacingMs: 100000 });   // spacing it will never finish
    await TICKS(15);
    assert.equal(calls(home).length, 1);
    // Join: present, so the arm comes back even though the job still waits.
    const dev = await fakeDevice(lab, { pressMs: 10 });
    await TICKS(10);
    assert.equal(JSON.parse(fs.readFileSync(path.join(home, 'notify.json'), 'utf8')).armed, true, 'a present device re-arms');
    assert.equal(calls(home).length, 1, 'and sends nothing while it is there');
    await dev.stop();
    await TICKS(25);
    assert.equal(calls(home).length, 2, 'the device left with work still waiting');
  } finally { await lab.stop(); }
});

test('the floor holds a second notification back however the arming went', async () => {
  const home = tempHome({ ...NTFY, minIntervalMs: 3600000 });
  const lab = await startServer(home, { ...FAST, ...stubEnv(home) });
  try {
    await queue(lab, { repeats: 4, spacingMs: 100000 });
    await TICKS(15);
    assert.equal(calls(home).length, 1);
    const dev = await fakeDevice(lab, { pressMs: 10 });
    await TICKS(10);
    await dev.stop();
    await TICKS(25);
    assert.equal(calls(home).length, 1, 'inside the floor, a re-arm sends nothing');
  } finally { await lab.stop(); }
});

test('no notify block in config.json is off, stub sender or not (E-outward)', async () => {
  const home = tempHome();  // no notify key at all
  const lab = await startServer(home, { ...FAST, ...stubEnv(home) });
  try {
    await queue(lab);
    await TICKS(25);
    assert.deepEqual(calls(home), []);
    const s = await api(lab, '/status').then((r) => r.json());
    assert.equal(s.notify.configured, false);
    assert.equal(s.notify.kind, null);
    const r = await api(lab, '/notify/test', { method: 'POST', body: '' });
    assert.equal(r.status, 503);
    assert.equal((await r.json()).configured, false);
    assert.ok(!fs.existsSync(path.join(home, 'notify.json')), 'nothing was written');
  } finally { await lab.stop(); }
});

test('the config a brand-new home gets carries no notify block, and no default names an address', async () => {
  // E-outward as a file check. `https://ntfy.sh` is a default HOST and reaches
  // nobody: without a topic — which is the address, and is Yona's — the block
  // is refused outright, so an unconfigured lab cannot notify anyone.
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'emu-lab-notify-fresh-'));
  const lab = await startServer(home, FAST);
  await lab.stop();
  const written = JSON.parse(fs.readFileSync(path.join(home, 'config.json'), 'utf8'));
  assert.equal(written.notify, undefined, 'a fresh config.json carries no notify block');
  assert.equal(readNotifyConfig({ kind: 'ntfy' }).cfg, null);
  assert.equal(readNotifyConfig({ kind: 'webhook' }).cfg, null);
});

test('notify test sends one on demand and does not spend the arm', async () => {
  const home = tempHome(NTFY);
  const lab = await startServer(home, { ...FAST, ...stubEnv(home) });
  try {
    const r = await api(lab, '/notify/test', { method: 'POST', body: '' });
    assert.equal(r.status, 200);
    const body = await r.json();
    assert.equal(body.ok, true);
    assert.equal(body.kind, 'ntfy');
    assert.equal(calls(home).length, 1);
    assert.equal(calls(home)[0].split('|')[0], 'emu-lab: test');
    // Still armed: a test must not cost the real notification.
    const j = await queue(lab);
    await TICKS(20);
    const lines = calls(home);
    assert.equal(lines.length, 2);
    assert.ok(lines[1].includes(j.id));
  } finally { await lab.stop(); }
});

test('a mistyped notify block is off and says why, rather than crash-looping a machine-wide service', () => {
  assert.deepEqual(readNotifyConfig(undefined), { cfg: null, why: null });
  assert.match(readNotifyConfig({ kind: 'webpush' }).why, /kind must be/);
  assert.match(readNotifyConfig({ kind: 'ntfy' }).why, /needs a topic/);
  assert.match(readNotifyConfig({ kind: 'ntfy', topic: 'has spaces' }).why, /needs a topic/);
  assert.match(readNotifyConfig({ kind: 'ntfy', topic: 'abcd', server: 'http://ntfy.sh' }).why, /must be https/);
  assert.match(readNotifyConfig({ kind: 'webhook', url: 'http://x.test/hook' }).why, /https url/);
  assert.match(readNotifyConfig({ kind: 'ntfy', topic: 'abcd', clickUrl: 'http://x.test/' }).why, /clickUrl must be https/);
  assert.match(readNotifyConfig({ kind: 'ntfy', topic: 'abcd', graceMs: -1 }).why, /graceMs/);
  const ok = readNotifyConfig({ kind: 'ntfy', topic: 'abcd_EFG-1' }).cfg;
  assert.equal(ok.target, 'https://ntfy.sh/abcd_EFG-1');
  assert.equal(ok.graceMs, 30000);
  assert.equal(ok.minIntervalMs, 600000);
});
