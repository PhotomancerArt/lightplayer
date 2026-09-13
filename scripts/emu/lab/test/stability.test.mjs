// F3: the stability stopping rule — the arithmetic against the eleven real
// runs the lab had produced by 2026-09-13 (`fixtures/real-runs.json`), and the
// scheduler's behaviour against the fake device.
//
// The sizing question the milestone asked, answered in `sizes the rule…` below:
// over those runs, the interpreter control row's best settles to within 5 % of
// its final best by press 3 at the latest. The window rule with
// `{row: 'interp', pct: 5, minPresses: 3}` is therefore the default — and the
// last test in the pure section records the one case where a settled control
// row is NOT a settled translated row, which is why the README says this rule
// is for exploration and not for a gate.
'use strict';

import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { computeReport, stabilityVerdict, controlRowKeys, renderReportMd, DEFAULT_STOP_WHEN_STABLE } from '../report.mjs';
import { startServer } from './helpers.mjs';
import { fakeDevice } from './fake-device.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REAL = JSON.parse(fs.readFileSync(path.join(HERE, 'fixtures', 'real-runs.json'), 'utf8')).runs;

// --- the arithmetic ---------------------------------------------------------

/// A report built from one fixture run's first `n` presses, so the verdict can
/// be asked the way the server asks it: after each press, from the report.
function repFor(run, n, rowFilter = null) {
  const presses = [];
  for (let i = 0; i < n; i++) {
    const results = Object.entries(run.rows)
      .filter(([k]) => !rowFilter || rowFilter.includes(k))
      .filter(([, seq]) => seq[i] !== null && seq[i] !== undefined)
      .map(([key, seq]) => {
        const [slug, grade, mode, fn] = key.split('/');
        return { slug, grade, mode, fnBlocks: mode === 'jit' ? Number(fn) : null, realtime: seq[i], nsPerInstr: 10, uartSha256: 'deadbeefdeadbeef' };
      });
    presses.push({ n: i + 1, build: run.build, state: 'done', tainted: false, taintReasons: [], results });
  }
  return computeReport({ id: run.job, builds: [run.build], rows: 'gate-rows', repeats: run.repeats, spacingMs: 0, state: 'running' }, presses);
}

/// The press at which the rule would stop this run, or null.
function stopsAt(run, opt) {
  const n = Math.max(...Object.values(run.rows).map((s) => s.length));
  for (let k = 1; k <= n; k++) {
    const v = stabilityVerdict(repFor(run, k), run.build, opt);
    if (v.stable) return k;
  }
  return null;
}

test('the default is the milestone\'s shape: interp, 5 %, last 3', () => {
  assert.deepEqual(DEFAULT_STOP_WHEN_STABLE, { row: 'interp', pct: 5, minPresses: 3 });
});

test('controlRowKeys: "interp" is every interpreter row, "all" is every row, anything else is one exact key', () => {
  const run = REAL.find((r) => r.job === 'G-M7B');
  const rep = repFor(run, 5);
  assert.deepEqual(controlRowKeys(rep, run.build, 'interp'), ['render-basic/t2/interp']);
  assert.equal(controlRowKeys(rep, run.build, 'all').length, 4);
  assert.deepEqual(controlRowKeys(rep, run.build, 'render-basic/t2/jit/8'), ['render-basic/t2/jit/8']);
  assert.deepEqual(controlRowKeys(rep, run.build, 'render-basic/t2/jit/64'), [], 'a row the job never produced matches nothing, so the rule never fires');
});

test('sizes the rule: over the eleven real runs the control row\'s best settles within 5 % by press 3 at the latest', () => {
  // "after how many presses does its best stop moving by more than x %": the
  // smallest k whose running best is within x % of the run's final best.
  const settle = (seq, pct) => {
    const vals = seq.filter((x) => x !== null);
    const finalBest = Math.max(...vals);
    for (let k = 1; k <= vals.length; k++) {
      const b = Math.max(...vals.slice(0, k));
      if (((finalBest - b) / finalBest) * 100 <= pct) return k;
    }
    return null;
  };
  const rows = [];
  for (const run of REAL) {
    for (const [key, seq] of Object.entries(run.rows)) {
      if (key.split('/')[2] !== 'interp') continue;
      rows.push({ job: run.job, build: run.build, key, at3: settle(seq, 3), at5: settle(seq, 5), at10: settle(seq, 10) });
    }
  }
  assert.equal(rows.length, 12, 'eleven runs, twelve interpreter control rows (one job benches two slugs)');
  assert.equal(Math.max(...rows.map((r) => r.at3)), 3, '3 %: every control row settles by press 3');
  assert.equal(Math.max(...rows.map((r) => r.at5)), 3, '5 %: every control row settles by press 3');
  assert.equal(Math.max(...rows.map((r) => r.at10)), 2, '10 %: every control row settles by press 2');
  // minPresses = 3 is that number; pct = 5 is the middle column.
  assert.equal(DEFAULT_STOP_WHEN_STABLE.minPresses, 3);
  assert.equal(DEFAULT_STOP_WHEN_STABLE.pct, 5);
});

test('the plan\'s acceptance number: on G1 the default rule does not stop the P3 head before press 3', () => {
  const g1 = REAL.find((r) => r.job === 'j-20260912-0158-f164' && r.build === '86cb2e0');
  assert.deepEqual(g1.rows['render-basic/t2/interp'], [0.586, 0.534, 0.527, 0.476, 0.519]);
  for (const k of [1, 2]) {
    assert.equal(stabilityVerdict(repFor(g1, k), g1.build, DEFAULT_STOP_WHEN_STABLE).stable, false, 'never stable before minPresses values exist (press ' + k + ')');
  }
  // At press 3 the window is 0.586/0.534/0.527 — 10.1 % apart, over 5 %.
  const v3 = stabilityVerdict(repFor(g1, 3), g1.build, DEFAULT_STOP_WHEN_STABLE);
  assert.equal(v3.stable, false);
  assert.match(v3.why, /last 3 \(0\.586 0\.534 0\.527\) spread 10\.1 % > 5 %/);
  assert.equal(stopsAt(g1, DEFAULT_STOP_WHEN_STABLE), null, 'the default never stops this run: it never settles in five presses');
  // At 10 % it stops, and even then not before press 3 — press 5.
  assert.equal(stopsAt(g1, { row: 'interp', pct: 10, minPresses: 3 }), 5);
});

test('a run that is genuinely flat stops at press 3, and says why', () => {
  const s1 = REAL.find((r) => r.job === 'j-20260913-0755-1213');
  assert.equal(stopsAt(s1, DEFAULT_STOP_WHEN_STABLE), 3, 'S1, 3-minute spacing, interp 0.677 0.680 0.676 — 0.6 % apart');
  const v = stabilityVerdict(repFor(s1, 3), s1.build, DEFAULT_STOP_WHEN_STABLE);
  assert.equal(v.stable, true);
  assert.equal(v.afterPress, 3);
  assert.match(v.why, /render-basic t2 interpreter last 3 \(0\.677 0\.680 0\.676\) within 0\.6 % — at or under 5 % after 3 presses/);
  assert.deepEqual(v.rows, [{ key: 'render-basic/t2/interp', window: [0.677, 0.68, 0.676], spreadPct: 0.6 }]);
});

test('the rule over all eleven real runs: which stop where, at 3 / 5 / 10 %', () => {
  // The table the PR and the README quote, asserted so it cannot drift.
  const table = REAL.map((run) => [run.job + '/' + run.build, ...[3, 5, 10].map((pct) => stopsAt(run, { row: 'interp', pct, minPresses: 3 }))]);
  assert.deepEqual(table, [
    ['j-20260912-0158-f164/86cb2e0', null, null, 5],
    ['j-20260912-0158-f164/23a3d3c', null, 3, 3],
    ['j-20260912-1653-2111/23a3d3c', null, null, null],
    ['j-20260912-1718-5940/f8c44c2-dirty-d65f8c', null, 3, 3],
    ['j-20260912-1718-5940/f8c44c2-dirty-15a80d', null, 3, 3],
    ['j-20260913-0726-c5bb/9f67d78', null, null, 3],
    ['j-20260913-0755-1213/9f67d78', 3, 3, 3],
    ['j-20260913-0755-14e2/9f67d78', null, 4, 4],
    ['j-20260913-0758-2bb9/9f67d78', 5, 5, 3],
    ['j-20260913-0758-2bb9/0adccaa', 5, 5, 3],
    ['G-M7B/86cb2e0', 3, 3, 3],
  ]);
  // j-20260913-0726-c5bb benches two slugs, so "interp" is two control rows
  // and BOTH must settle — the conservative reading, and the one that keeps
  // render-rocaille from being stopped by render-basic.
  const two = REAL.find((r) => r.job === 'j-20260913-0726-c5bb');
  assert.equal(controlRowKeys(repFor(two, 5), two.build, 'interp').length, 2);
});

test('a settled control row is NOT a settled translated row: the cost the README warns about', () => {
  // P1b R0-vs-R1, the decision that turned on 1–2.7 %. The interpreter row is
  // flat by press 3 (0.648 0.627 0.646, 3.2 %) while jit/8 is still climbing
  // (0.939 0.972 0.973 → 1.064 on press 4). Stopping there quotes a best
  // 8.6 % under the five-press best.
  const r0 = REAL.find((r) => r.job === 'j-20260912-1718-5940' && r.build === 'f8c44c2-dirty-d65f8c');
  assert.equal(stopsAt(r0, DEFAULT_STOP_WHEN_STABLE), 3);
  const jit = r0.rows['render-basic/t2/jit/8'];
  const bestAt3 = Math.max(...jit.slice(0, 3));
  const bestAt5 = Math.max(...jit);
  assert.equal(bestAt3, 0.973);
  assert.equal(bestAt5, 1.064);
  assert.equal(Number((((bestAt5 - bestAt3) / bestAt5) * 100).toFixed(1)), 8.6);
  // `row: 'all'` does not save this one either — three flat presses at the
  // bottom of a ramp look exactly like three flat presses on a plateau — but
  // a longer window does.
  assert.equal(stopsAt(r0, { row: 'all', pct: 5, minPresses: 3 }), 3);
  assert.equal(stopsAt(r0, { row: 'all', pct: 5, minPresses: 4 }), null);
  assert.equal(stopsAt(r0, { row: 'interp', pct: 3, minPresses: 3 }), null);
});

test('excluded presses never count toward stability (the invariant)', () => {
  // Three identical values, but the middle one is tainted: only two count, so
  // the window of three cannot close.
  const run = { job: 'j', build: 'A', repeats: 5, rows: { 'render-basic/t2/interp': [0.60, 0.60, 0.60, 0.90, 0.60] } };
  const job = { id: 'j', builds: ['A'], rows: 'gate-rows', repeats: 5, spacingMs: 0, state: 'running' };
  const mk = (states) => states.map((st, i) => ({
    n: i + 1, build: 'A', state: st === 'tainted' ? 'done' : st, tainted: st === 'tainted', taintReasons: st === 'tainted' ? ['hidden'] : [],
    results: [{ slug: 'render-basic', grade: 't2', mode: 'interp', fnBlocks: null, realtime: run.rows['render-basic/t2/interp'][i], nsPerInstr: 10, uartSha256: 'aa' }],
  }));
  const clean = computeReport(job, mk(['done', 'done', 'done']));
  assert.equal(stabilityVerdict(clean, 'A', DEFAULT_STOP_WHEN_STABLE).stable, true);
  const tainted = computeReport(job, mk(['done', 'tainted', 'done']));
  const v = stabilityVerdict(tainted, 'A', DEFAULT_STOP_WHEN_STABLE);
  assert.equal(v.stable, false);
  assert.match(v.why, /2 of 3 presses counted/);
  const failed = computeReport(job, mk(['done', 'failed', 'done']));
  assert.equal(stabilityVerdict(failed, 'A', DEFAULT_STOP_WHEN_STABLE).stable, false);
  // And the window is the last three COUNTED values, not the last three
  // presses: the tainted press 2 is skipped over, not treated as a value.
  const five = computeReport(job, mk(['done', 'tainted', 'done', 'done', 'done']));
  const v5 = stabilityVerdict(five, 'A', DEFAULT_STOP_WHEN_STABLE);
  assert.equal(v5.stable, false, 'window 0.600/0.900/0.600 is 33 % apart');
  assert.match(v5.why, /\(0\.600 0\.900 0\.600\) spread 33\.3 %/);
});

test('a report carries stopWhenStable and stoppedEarly, and the .md says so', () => {
  const job = {
    id: 'j', builds: ['A'], rows: 'gate-rows', repeats: 5, spacingMs: 0, state: 'done',
    stopWhenStable: { row: 'interp', pct: 5, minPresses: 3 },
    stoppedEarly: [{ build: 'A', afterPress: 3, row: 'interp', pct: 5, minPresses: 3, skipped: 2, why: 'render-basic t2 interpreter last 3 (0.600 0.600 0.600) within 0.0 % — at or under 5 % after 3 presses', rows: [] }],
  };
  const presses = [1, 2, 3].map((n) => ({ n, build: 'A', state: 'done', tainted: false, taintReasons: [], results: [{ slug: 'render-basic', grade: 't2', mode: 'interp', fnBlocks: null, realtime: 0.6, nsPerInstr: 10, uartSha256: 'aa' }] }))
    .concat([4, 5].map((n) => ({ n, build: 'A', state: 'skipped', tainted: false, taintReasons: [], results: [] })));
  const rep = computeReport(job, presses);
  assert.deepEqual(rep.stopWhenStable, { row: 'interp', pct: 5, minPresses: 3 });
  assert.equal(rep.stoppedEarly.length, 1);
  // A skipped press is not an exclusion: it is not a hole in the sequence.
  assert.deepEqual(rep.excluded, []);
  assert.deepEqual(rep.perBuild.A.rows['render-basic/t2/interp'].seq, [0.6, 0.6, 0.6]);
  assert.equal(rep.presses.length, 5);
  assert.equal(rep.presses[4].state, 'skipped');
  const md = renderReportMd(rep);
  assert.match(md, /stopping rule: stop a build when its `interp` row's last 3 counted presses are within 5 % of each other/);
  assert.match(md, /\*\*stopped early: A after press 3 of 5\*\* — render-basic t2 interpreter last 3 .* \(2 presses not taken\)/);
  assert.match(md, /## A \(3 of 3 presses quoted\)/);
});

// --- the scheduler ----------------------------------------------------------

const FAST = { LAB_TICK_MS: '20', LAB_COOLDOWN_MS: '0', LAB_LOST_MS: '2000' };

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
const waitJob = async (id, timeout = 15) => { const r = await api('/wait?job=' + id + '&timeout=' + timeout); return { status: r.status, body: await r.json() }; };

// The rows a press answers with: a flat control row (stable at press 3) or a
// control row that never settles.
const rows = (interp, jit) => (b, n) => [
  { key: 'render-basic/t2/jit/8', realtime: jit ? jit[(n - 1) % jit.length] : 1.0 },
  { key: 'render-basic/t2/interp', realtime: interp[(n - 1) % interp.length] },
];

before(async () => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), 'emu-lab-stability-'));
  stageFixture(home, ['aaa1111', 'bbb2222']);
  lab = await startServer(home, FAST);
});
after(() => lab && lab.stop());

test('a stable table stops early at the sized press: 3 of 8 presses taken, 5 skipped, the report says so', async () => {
  // S1's real control row, repeated: 0.677 0.680 0.676 — 0.6 % apart at press 3.
  const dev = await fakeDevice(lab, { pressMs: 10, numbers: rows([0.677, 0.680, 0.676, 0.670, 0.665, 0.677, 0.684, 0.690]) });
  const { status, body: j } = await queue({ builds: ['aaa1111'], rows: 'gate-rows', repeats: 8, spacingMs: 0, ttlMs: 60000, stopWhenStable: { row: 'interp', pct: 5, minPresses: 3 } });
  assert.equal(status, 201);
  assert.deepEqual(j.stopWhenStable, { row: 'interp', pct: 5, minPresses: 3 });
  assert.equal(j.presses.length, 8, 'repeats is still the hard cap; the rule only stands presses down');
  const w = await waitJob(j.id);
  assert.equal(w.body.job.state, 'done');
  assert.equal(dev.answered.length, 3, 'the device was asked for three presses, not eight');
  assert.equal(w.body.job.presses.done, 3);
  assert.equal(w.body.job.presses.skipped, 5);
  const se = w.body.report.stoppedEarly;
  assert.equal(se.length, 1);
  assert.equal(se[0].build, 'aaa1111');
  assert.equal(se[0].afterPress, 3);
  assert.equal(se[0].skipped, 5);
  assert.match(se[0].why, /within 0\.6 %/);
  assert.deepEqual(w.body.report.perBuild.aaa1111.rows['render-basic/t2/interp'].seq, [0.677, 0.68, 0.676]);
  assert.deepEqual(w.body.report.excluded, [], 'a skipped press is not an excluded one');
  const md = fs.readFileSync(path.join(home, 'jobs', j.id, 'report.md'), 'utf8');
  assert.match(md, /\*\*stopped early: aaa1111 after press 3 of 8\*\*/);
  await dev.stop();
});

test('an unstable table runs to repeats and the report has no stoppedEarly', async () => {
  // G1's P3 head control row: never three presses within 5 %.
  const dev = await fakeDevice(lab, { pressMs: 10, numbers: rows([0.586, 0.534, 0.527, 0.476, 0.519]) });
  const { body: j } = await queue({ builds: ['bbb2222'], rows: 'gate-rows', repeats: 5, spacingMs: 0, ttlMs: 60000, stopWhenStable: { row: 'interp', pct: 5, minPresses: 3 } });
  const w = await waitJob(j.id);
  assert.equal(w.body.job.state, 'done');
  assert.equal(dev.answered.length, 5);
  assert.equal(w.body.job.presses.skipped, 0);
  assert.equal(w.body.report.stoppedEarly, null);
  assert.equal(w.body.report.perBuild.bbb2222.rows['render-basic/t2/interp'].n, 5);
  assert.doesNotMatch(fs.readFileSync(path.join(home, 'jobs', j.id, 'report.md'), 'utf8'), /stopped early/);
  await dev.stop();
});

test('an A/B stops per build: A settles at press 3, B keeps interleaving to its cap', async () => {
  // A is flat; B wanders. The interleave must keep running for B alone.
  const flat = [0.677, 0.680, 0.676, 0.670, 0.665, 0.677];
  const wander = [0.586, 0.534, 0.527, 0.476, 0.519, 0.610];
  const dev = await fakeDevice(lab, {
    pressMs: 10,
    numbers: (b, n) => {
      // Per-build ordinal: presses interleave A1 B1 A2 B2 …
      const i = Math.floor((n - 1) / 2);
      return rows(b === 'aaa1111' ? flat : wander)(b, i + 1);
    },
  });
  const { body: j } = await queue({ builds: ['aaa1111', 'bbb2222'], rows: 'gate-rows', repeats: 6, spacingMs: 0, ttlMs: 60000, stopWhenStable: { row: 'interp', pct: 5, minPresses: 3 } });
  assert.equal(j.presses.length, 12);
  const w = await waitJob(j.id);
  assert.equal(w.body.job.state, 'done');
  const byBuild = (b) => dev.answered.filter((a) => a.build === b).length;
  assert.equal(byBuild('aaa1111'), 3, 'A stopped after its third press');
  assert.equal(byBuild('bbb2222'), 6, 'B ran to repeats');
  const se = w.body.report.stoppedEarly;
  assert.equal(se.length, 1);
  assert.equal(se[0].build, 'aaa1111');
  assert.equal(se[0].afterPress, 3);
  // The A/B arithmetic still has both builds; A just has fewer presses.
  assert.equal(w.body.report.perBuild.aaa1111.rows['render-basic/t2/interp'].n, 3);
  assert.equal(w.body.report.perBuild.bbb2222.rows['render-basic/t2/interp'].n, 6);
  assert.ok(w.body.report.ab['render-basic/t2/jit/8'], 'an A/B report is still an A/B report');
  // And the interleave kept its shape for as long as A was in it.
  assert.deepEqual(dev.answered.slice(0, 6).map((a) => a.build), ['aaa1111', 'bbb2222', 'aaa1111', 'bbb2222', 'aaa1111', 'bbb2222']);
  assert.deepEqual(dev.answered.slice(6).map((a) => a.build), ['bbb2222', 'bbb2222', 'bbb2222']);
  await dev.stop();
});

test('the field is validated, and absent it changes nothing', async () => {
  const cases = [
    [{ stopWhenStable: 'yes' }, /must be an object/],
    [{ stopWhenStable: { pct: 0 } }, /pct must be > 0/],
    [{ stopWhenStable: { pct: 80 } }, /pct must be > 0 and ≤ 50/],
    [{ stopWhenStable: { minPresses: 1 } }, /minPresses must be ≥ 2/],
    [{ stopWhenStable: { minPresses: 9 } }, /more than repeats \(3\): the rule could never fire/],
    [{ stopWhenStable: { row: '' } }, /row must be "interp", "all", or a row key/],
  ];
  for (const [extra, re] of cases) {
    const r = await queue({ builds: ['aaa1111'], rows: 'gate-rows', repeats: 3, spacingMs: 0, ttlMs: 60000, ...extra });
    assert.equal(r.status, 400, JSON.stringify(extra));
    assert.match(r.body.error, re);
  }
  // An explicit row list with no interpreter row cannot ever satisfy the rule.
  const noInterp = await queue({
    builds: ['aaa1111'], rows: [{ slug: 'render-basic', grade: 't2', mode: 'jit', fnBlocks: 8 }], repeats: 3, spacingMs: 0, ttlMs: 60000,
    stopWhenStable: {},
  });
  assert.equal(noInterp.status, 400);
  assert.match(noInterp.body.error, /matches none of this job's rows \(render-basic\/t2\/jit\/8\)/);
  // …but naming that row explicitly is fine.
  const ok = await queue({
    builds: ['aaa1111'], rows: [{ slug: 'render-basic', grade: 't2', mode: 'jit', fnBlocks: 8 }], repeats: 3, spacingMs: 0, ttlMs: 60000,
    stopWhenStable: { row: 'render-basic/t2/jit/8' },
  });
  assert.equal(ok.status, 201);
  assert.deepEqual(ok.body.stopWhenStable, { row: 'render-basic/t2/jit/8', pct: 5, minPresses: 3 });
  await api('/jobs/' + ok.body.id, { method: 'DELETE' });
  // Absent: null on the job, null in the report, every press taken.
  const off = await queue({ builds: ['aaa1111'], rows: 'gate-rows', repeats: 2, spacingMs: 0, ttlMs: 60000 });
  assert.equal(off.body.stopWhenStable, null);
  assert.deepEqual(off.body.stoppedEarly, []);
  const dev = await fakeDevice(lab, { pressMs: 10, numbers: rows([0.6, 0.6, 0.6]) });
  const w = await waitJob(off.body.id);
  assert.equal(w.body.job.presses.done, 2);
  assert.equal(w.body.job.presses.skipped, 0);
  assert.equal(w.body.report.stopWhenStable, null);
  assert.equal(w.body.report.stoppedEarly, null);
  await dev.stop();
});

test('a tainted press cannot be the press that declares a build stable', async () => {
  // Presses 1–3 are identical, but press 2 is tainted: the retry press 4 is
  // what closes the window, so four presses are taken, not three.
  const dev = await fakeDevice(lab, { pressMs: 10, taintOn: new Set([2]), numbers: rows([0.6, 0.6, 0.6, 0.6, 0.6]) });
  const { body: j } = await queue({ builds: ['aaa1111'], rows: 'gate-rows', repeats: 5, spacingMs: 0, ttlMs: 60000, retryTainted: 1, stopWhenStable: { row: 'interp', pct: 5, minPresses: 3 } });
  const w = await waitJob(j.id);
  assert.equal(w.body.job.state, 'done');
  assert.equal(dev.answered.length, 4, 'presses 1,2(tainted),3 then the retry 6 — four taken');
  assert.equal(w.body.report.stoppedEarly[0].afterPress, 4, 'four presses taken, three of them counted');
  assert.deepEqual(w.body.report.excluded, [{ press: 2, build: 'aaa1111', reason: ['hidden'] }]);
  assert.equal(w.body.report.perBuild.aaa1111.rows['render-basic/t2/interp'].n, 3);
  await dev.stop();
});
