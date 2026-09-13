// P3: the DD41 arithmetic against the G-M7B five-press numbers.
'use strict';

import { test } from 'node:test';
import assert from 'node:assert/strict';

import { rowKey, computeReport, renderReportMd } from '../report.mjs';

// The G-M7B gate table (2026-09-11, iPhone 16 Pro Max, P3 merge 86cb2e02c):
// five presses, four rows each.
const G_M7B = {
  'render-basic/t2/jit/8': [0.863, 0.945, 1.008, 0.921, 0.818],
  'render-basic/t2/jit/16': [0.889, 0.888, 0.926, 0.780, 0.761],
  'render-basic/t2/jit/32': [0.781, 0.761, 0.801, 0.707, 0.675],
  'render-basic/t2/interp': [0.618, 0.626, 0.613, 0.585, 0.500],
};

function pressesFor(build, table, n = 5, opts = {}) {
  const out = [];
  for (let i = 0; i < n; i++) {
    const results = Object.entries(table).map(([key, seq]) => {
      const [slug, grade, mode, fn] = key.split('/');
      return { slug, grade, mode, fnBlocks: mode === 'jit' ? Number(fn) : null, realtime: seq[i], nsPerInstr: 10, uartSha256: opts.sha ? opts.sha(i) : '2407828f80684331' };
    });
    out.push({ n: i + 1, build, state: opts.failOn?.has(i + 1) ? 'failed' : 'done', tainted: !!opts.taintOn?.has(i + 1), taintReasons: opts.taintOn?.has(i + 1) ? ['hidden'] : [], results });
  }
  return out;
}

test('rowKey names a translated row by its blocks/fn and the interpreter row by mode', () => {
  assert.equal(rowKey({ slug: 'render-basic', grade: 't2', mode: 'jit', fnBlocks: 8 }), 'render-basic/t2/jit/8');
  assert.equal(rowKey({ slug: 'render-basic', grade: 't2', mode: 'interp', fnBlocks: null }), 'render-basic/t2/interp');
});

test('the G-M7B five presses reproduce best 1.008 (press 3), spread 18.8 %, ratio 1.40 1.51 1.64 1.57 1.64 (best 1.64)', () => {
  const job = { id: 'j-test', builds: ['86cb2e0'], rows: 'gate-rows', repeats: 5, spacingMs: 180000, state: 'done', boundDevice: 'd1', boundDeviceName: 'phone' };
  const rep = computeReport(job, pressesFor('86cb2e0', G_M7B));
  const r8 = rep.perBuild['86cb2e0'].rows['render-basic/t2/jit/8'];
  assert.deepEqual(r8.seq, [0.863, 0.945, 1.008, 0.921, 0.818]);
  assert.equal(r8.best, 1.008);
  assert.equal(r8.bestPress, 3);
  assert.equal(r8.median, 0.921);
  assert.equal(r8.spreadPct, 18.8); // (1.008 − 0.818) / 1.008
  const ratio = rep.perBuild['86cb2e0'].ratio['render-basic/t2/jit/8'];
  assert.deepEqual(ratio.seq, [1.40, 1.51, 1.64, 1.57, 1.64]);
  assert.equal(ratio.best, 1.64);
  assert.equal(ratio.median, 1.57);
  assert.equal(rep.perBuild['86cb2e0'].ratio['render-basic/t2/interp'], undefined, 'no ratio for the interpreter itself');
  assert.equal(rep.perBuild['86cb2e0'].identity.consistent, true);
  assert.equal(rep.perBuild['86cb2e0'].identity.uartSha256, '2407828f80684331');
  assert.deepEqual(rep.excluded, []);
  assert.equal(rep.ab, null);
  const md = renderReportMd(rep);
  assert.match(md, /\| render-basic t2 8\/fn \| 0\.863 \| 0\.945 \| 1\.008 \| 0\.921 \| 0\.818 \| \*\*1\.008×\*\* \(p3\) \| 0\.921 \| 18\.8 % \|/);
  assert.match(md, /÷ interpreter, same press\*\* \| 1\.40 \| 1\.51 \| 1\.64 \| 1\.57 \| 1\.64 \| \*\*1\.64×\*\*/);
  assert.match(md, /UART `2407828f80684331`/);
});

test('a tainted press is excluded: null in seq, listed, best over the rest', () => {
  const job = { id: 'j', builds: ['A'], rows: 'gate-rows', repeats: 5, spacingMs: 0, state: 'done' };
  const rep = computeReport(job, pressesFor('A', G_M7B, 5, { taintOn: new Set([3]) }));
  const r8 = rep.perBuild.A.rows['render-basic/t2/jit/8'];
  assert.deepEqual(r8.seq, [0.863, 0.945, null, 0.921, 0.818]);
  assert.equal(r8.best, 0.945);
  assert.equal(r8.bestPress, 2);
  assert.equal(r8.n, 4);
  assert.deepEqual(rep.excluded, [{ press: 3, build: 'A', reason: ['hidden'] }]);
  assert.match(renderReportMd(rep), /excluded: press 3 \(hidden\)/);
  const failed = computeReport(job, pressesFor('A', G_M7B, 5, { failOn: new Set([5]) }));
  assert.deepEqual(failed.excluded, [{ press: 5, build: 'A', reason: ['failed'] }]);
  assert.equal(failed.perBuild.A.rows['render-basic/t2/jit/8'].seq[4], null);
});

test('an A/B report interleaves presses per build and reports best/median/ratio deltas', () => {
  const job = { id: 'j', builds: ['A', 'B'], rows: 'gate-rows', repeats: 2, spacingMs: 0, state: 'done' };
  const B = { 'render-basic/t2/jit/8': [1.1, 1.2], 'render-basic/t2/interp': [0.5, 0.5] };
  const A = { 'render-basic/t2/jit/8': [1.0, 1.0], 'render-basic/t2/interp': [0.5, 0.5] };
  const pa = pressesFor('A', A, 2), pb = pressesFor('B', B, 2);
  // Global press numbers A1 B1 A2 B2.
  const presses = [{ ...pa[0], n: 1 }, { ...pb[0], n: 2 }, { ...pa[1], n: 3 }, { ...pb[1], n: 4 }];
  const rep = computeReport(job, presses);
  assert.deepEqual(rep.perBuild.A.rows['render-basic/t2/jit/8'].seq, [1.0, 1.0]);
  assert.deepEqual(rep.perBuild.B.rows['render-basic/t2/jit/8'].seq, [1.1, 1.2]);
  assert.equal(rep.ab['render-basic/t2/jit/8'].bestDelta, 0.2);
  assert.equal(rep.ab['render-basic/t2/jit/8'].medianDelta, 0.15);
  assert.equal(rep.ab['render-basic/t2/jit/8'].medianRatioDelta, 0.15);
  assert.match(renderReportMd(rep), /## A\/B: B over A/);
  assert.match(renderReportMd(rep), /\| render-basic t2 8\/fn \| \+20\.0 % \| \+15\.0 % \| \+15\.0 % \|/);
});

test('identity: two different uart shas across a build is inconsistent; null is unknown', () => {
  const job = { id: 'j', builds: ['A'], rows: 'gate-rows', repeats: 2, spacingMs: 0, state: 'done' };
  const bad = computeReport(job, pressesFor('A', G_M7B, 2, { sha: (i) => (i === 0 ? 'aaaa' : 'bbbb') }));
  assert.equal(bad.perBuild.A.identity.consistent, false);
  assert.match(renderReportMd(bad), /INCONSISTENT/);
  const unknown = computeReport(job, pressesFor('A', G_M7B, 2, { sha: (i) => (i === 0 ? 'aaaa' : null) }));
  assert.equal(unknown.perBuild.A.identity.consistent, true);
});
