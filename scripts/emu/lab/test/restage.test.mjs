// The auto-restage follow-up: the lock that keeps two stages off one desk.
//
// A stage is minutes of cargo and the restage timer keeps firing under it, so
// the only thing standing between "10-minute timer" and "two concurrent
// builds" is the lock under `$LAB_HOME/.stage/`. These tests take it by hand
// (the test runner's own pid is the live holder) and check both sides of it
// without building anything: no network, no cargo, sub-second.
'use strict';

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const LAB = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');

/// A lab home with just enough in it for `need_home`: no server, no builds.
function tempHome() {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'emu-lab-restage-'));
  fs.writeFileSync(path.join(home, 'config.json'), JSON.stringify({ port: 0 }));
  fs.writeFileSync(path.join(home, 'token'), 'f'.repeat(32) + '\n');
  return home;
}

function takeLock(home, pid, what) {
  const dir = path.join(home, '.stage', 'lock');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'pid'), String(pid) + '\n');
  fs.writeFileSync(path.join(dir, 'what'), what + '\n');
  return dir;
}

function run(script, args, home) {
  return spawnSync('bash', [path.join(LAB, script), ...args], {
    env: { ...process.env, LAB_HOME: home },
    encoding: 'utf8',
  });
}

test('lab.sh stage refuses (3) while another stage holds the lock, and touches nothing', () => {
  const home = tempHome();
  takeLock(home, process.pid, 'deadbee');
  const r = run('lab.sh', ['stage', 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeef'], home);
  assert.equal(r.status, 3, r.stderr);
  assert.match(r.stderr, /already running \(pid \d+, deadbee\)/);
  // The refusal is before any git, any worktree and any build; and it left
  // the holder's lock exactly as it found it.
  assert.equal(fs.readFileSync(path.join(home, '.stage', 'lock', 'pid'), 'utf8').trim(), String(process.pid));
  assert.ok(!fs.existsSync(path.join(home, 'builds')));
});

test('a lock whose pid is gone is cleared, and the stage releases the lock it takes', () => {
  const home = tempHome();
  // A pid that cannot be alive: a `kill -9` mid-stage leaves exactly this.
  takeLock(home, 2147483646, 'gone');
  const r = run('lab.sh', ['stage', 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeef'], home);
  assert.match(r.stderr, /clearing a stale stage lock/);
  // It got past the lock and died on the sha instead — which is the proof it
  // took the lock rather than refusing it.
  assert.equal(r.status, 1, r.stderr);
  assert.match(r.stderr, /is not a commit/);
  assert.ok(!fs.existsSync(path.join(home, '.stage', 'lock')), 'the lock outlived the stage');
});

test('restage-main.sh logs one busy line and exits 0 while a stage runs', () => {
  const home = tempHome();
  takeLock(home, process.pid, 'abc1234');
  const r = run('restage-main.sh', [], home);
  assert.equal(r.status, 0, r.stderr);
  // One line per run, skips included — and it never reached the fetch.
  const lines = fs.readFileSync(path.join(home, 'log', 'restage.log'), 'utf8').trim().split('\n');
  assert.equal(lines.length, 1);
  assert.match(lines[0], /^\d{4}-\d\d-\d\dT[\d:]+Z busy: a stage is already running \(pid \d+, abc1234\)$/);
});

test('restage-main.sh says so and fails when there is no lab home', () => {
  const home = tempHome();
  fs.rmSync(path.join(home, 'config.json'));
  const r = run('restage-main.sh', [], home);
  assert.equal(r.status, 1);
  assert.match(fs.readFileSync(path.join(home, 'log', 'restage.log'), 'utf8'), /no lab at /);
});
