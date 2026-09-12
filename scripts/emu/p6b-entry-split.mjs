#!/usr/bin/env node
// M7 P6b: the module side of an entry, split from the per-instruction cost,
// inside ONE recording.
//
//   node scripts/emu/p6b-entry-split.mjs <recording-dir> <module.wasm> [seconds]
//   bun  scripts/emu/p6b-entry-split.mjs <recording-dir> <module.wasm> [seconds]
//
// G-M7P split `E` (per entry) from `T` (per translated instruction) by solving
// two images that differ 4.5x in mean stay length. That works, and it prices
// the WHOLE entry — the hart's re-entry path and the module's prologue
// together — because it is fitted to a whole run's wall clock.
//
// This fits the same two costs to the **module alone**, from one recording, so
// the difference between the two numbers is the hart side. It can do that
// without a second image because a recording is not one homogeneous thing: cut
// it into windows of `W` consecutive entries and the windows differ by several
// times in instructions per entry all by themselves. Each window is timed on
// its own — one `performance.now()` pair per W entries, so the clock is under
// a percent of what it measures — and the windows are then a least-squares
// system in three unknowns:
//
//   ns(window) = E x entries + T x instructions + C x import calls
//
// `C` is in there because an `mmio_load` or `mmio_store` is a JS frame in this
// harness and windows differ in how many they make; leaving it out would let
// MMIO-heavy windows masquerade as expensive entries. It is reported so the
// reader can see how much of the fit it is carrying.
//
// Entries still run **in recorded order** — the deltas are applied per entry
// and the identity check is the first pass — so cutting into windows changes
// nothing about what runs. Only the stopwatch moves.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { loadavg } from 'node:os';

const dir = process.argv[2];
const modulePath = process.argv[3];
const seconds = Number(process.argv[4] ?? 6);
const W = Number(process.argv[5] ?? 200);
if (!dir || !modulePath) {
  console.error('usage: p6b-entry-split.mjs <recording-dir> <module.wasm> [seconds] [window]');
  process.exit(2);
}

const meta = JSON.parse(readFileSync(join(dir, 'meta.json'), 'utf8'));
const wasm = readFileSync(modulePath);

function readImage() {
  const buf = readFileSync(join(dir, 'memory.bin'));
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  const out = [];
  let at = 0;
  while (at < buf.byteLength) {
    out.push({ offset: view.getUint32(at, true), bytes: buf.subarray(at + 8, at + 8 + view.getUint32(at + 4, true)) });
    at += 8 + view.getUint32(at + 4, true);
  }
  return out;
}

const CALL_BYTES = 40;

function readEntries() {
  const buf = readFileSync(join(dir, 'entries.bin'));
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  const out = [];
  let at = 0;
  const u32 = () => { const v = view.getUint32(at, true); at += 4; return v; };
  const i32 = () => { const v = view.getInt32(at, true); at += 4; return v; };
  const u64 = () => { const v = view.getBigUint64(at, true); at += 8; return v; };
  while (at < buf.byteLength) {
    const entry = u32(), cycleIn = u64(), instretIn = u64(), end = u64(), watchLo = u64(), watchHi = u64();
    const regsIn = new Int32Array(31);
    for (let i = 0; i < 31; i++) regsIn[i] = i32();
    const delta = [];
    for (let n = u32(); n > 0; n--) { const offset = u32(), len = u32(); delta.push({ offset, bytes: buf.subarray(at, at + len) }); at += len; }
    const callCount = u32();
    const calls = new DataView(buf.buffer, buf.byteOffset + at, callCount * CALL_BYTES);
    at += callCount * CALL_BYTES;
    const exitPc = u32(), flags = i32(), cycleOut = u64(), instretOut = u64();
    const regs = new Int32Array(31);
    for (let i = 0; i < 31; i++) regs[i] = i32();
    out.push({ entry, cycleIn, instretIn, end, watchLo, watchHi, regsIn, delta, calls, callCount, exitPc, flags, cycleOut, instretOut, regs });
  }
  return out;
}

const EXCHANGE_CYCLE = 128, EXCHANGE_INSTRET = 136, EXCHANGE_FLAGS = 144, EXCHANGE_CROSS = 152;
const X = meta.exchange;

const image = readImage();
const entries = readEntries();
const memory = new WebAssembly.Memory({ initial: meta.pages });
const bytes = new Uint8Array(memory.buffer);
const view = new DataView(memory.buffer);
const regFile = new Int32Array(memory.buffer, X, 32);
const restore = () => { for (const r of image) bytes.set(r.bytes, r.offset); };

let current = null, cursor = 0;
function nextCall(kind, pc, address) {
  const at = cursor * CALL_BYTES;
  cursor++;
  const c = current.calls;
  if (c.getUint8(at) !== kind || c.getUint32(at + 4, true) !== (pc >>> 0) || c.getUint32(at + 16, true) !== (address >>> 0)) {
    throw new Error(`entry ${current.entry}, call ${cursor - 1}: the module and the recording disagree`);
  }
  return at;
}
const imports = { emu: {
  memory,
  mmio_load: (pc, _c, address) => current.calls.getBigUint64(nextCall(0, pc, address) + 32, true),
  // i64 since M7b P2 gave the store its own status word; a bare pc makes V8
  // answer `TypeError: Cannot convert <pc> to a BigInt` from inside the module.
  mmio_store: (pc, _c, address) => current.calls.getBigUint64(nextCall(1, pc, address) + 32, true),
  // M7b P2's fourth import. Recorded with `address` zero, answering
  // `(status << 32) | pc` exactly as a store does.
  poll: (pc) => current.calls.getBigUint64(nextCall(2, pc, 0) + 32, true),
  step_one: (pc) => { throw new Error(`the escape hatch fired at pc ${(pc >>> 0).toString(16)}`); },
} };

const t0 = performance.now();
const mod = new WebAssembly.Module(wasm);
const compileMs = performance.now() - t0;
const instance = new WebAssembly.Instance(mod, imports);
const run = instance.exports.run;

/** One entry, exactly as `jit-image-bench.mjs` runs it. */
function one(e) {
  for (const d of e.delta) bytes.set(d.bytes, d.offset);
  regFile.set(e.regsIn, 1);
  regFile[0] = 0;
  view.setBigUint64(X + EXCHANGE_CYCLE, e.cycleIn, true);
  view.setBigUint64(X + EXCHANGE_INSTRET, e.instretIn, true);
  current = e;
  cursor = 0;
  return run(e.entry, e.cycleIn, e.instretIn, e.end, e.watchLo, e.watchHi);
}

// `P6B_NO_RUN=1` does everything an entry does EXCEPT call the module: the
// delta writes, the register file, the two counters. That is this harness's
// own per-entry cost, and subtracting it is what turns the fit's `E` from "an
// entry in this JavaScript rig" into "an entry in the module". It is not a
// correct replay — nothing advances — so it skips the identity pass and says
// so in its output.
const noRun = process.env.P6B_NO_RUN === '1';
function onePrep(e) {
  for (const d of e.delta) bytes.set(d.bytes, d.offset);
  regFile.set(e.regsIn, 1);
  regFile[0] = 0;
  view.setBigUint64(X + EXCHANGE_CYCLE, e.cycleIn, true);
  view.setBigUint64(X + EXCHANGE_INSTRET, e.instretIn, true);
  current = e;
  cursor = 0;
}
const step = noRun ? onePrep : one;

// The identity pass: a fast wrong number is worse than no number, so the same
// full check `jit-image-bench.mjs` makes runs once before any stopwatch.
restore();
let retiredCheck = 0n;
let crossTotal = 0n;
for (const e of noRun ? [] : entries) {
  const pc = one(e);
  if ((pc >>> 0) !== e.exitPc) throw new Error(`entry ${e.entry}: left at ${(pc >>> 0).toString(16)}, recorded ${e.exitPc.toString(16)}`);
  const cycle = view.getBigUint64(X + EXCHANGE_CYCLE, true);
  const instret = view.getBigUint64(X + EXCHANGE_INSTRET, true);
  if (cycle !== e.cycleOut || instret !== e.instretOut) throw new Error(`entry ${e.entry}: counters disagree`);
  if ((view.getInt32(X + EXCHANGE_FLAGS, true) & 3) !== (e.flags & 3)) throw new Error(`entry ${e.entry}: flags disagree`);
  for (let i = 0; i < 31; i++) if (regFile[i + 1] !== e.regs[i]) throw new Error(`entry ${e.entry}: x${i + 1} disagrees`);
  if (cursor !== e.callCount) throw new Error(`entry ${e.entry}: ${cursor} import calls, recorded ${e.callCount}`);
  retiredCheck += e.instretOut - e.instretIn;
  crossTotal += view.getBigUint64(X + EXCHANGE_CROSS, true);
}
if (!noRun && retiredCheck !== BigInt(meta.retired)) throw new Error(`replay retired ${retiredCheck}, recording says ${meta.retired}`);

// The windows, and what is constant about each of them.
const windows = [];
for (let i = 0; i < entries.length; i += W) {
  const slice = entries.slice(i, i + W);
  let instr = 0, calls = 0;
  for (const e of slice) { instr += Number(e.instretOut - e.instretIn); calls += e.callCount; }
  windows.push({ from: i, slice, n: slice.length, instr, calls, ns: 0 });
}

/** One pass over the recording, timing each window on its own. */
function pass() {
  restore();
  for (const w of windows) {
    const t = performance.now();
    for (const e of w.slice) step(e);
    w.ns += (performance.now() - t) * 1e6;
  }
}

// Warm up past the tiering transient, then measure.
const warm = performance.now();
while (performance.now() - warm < 1500) pass();
for (const w of windows) w.ns = 0;
let passes = 0;
const started = performance.now();
while (performance.now() - started < seconds * 1000) { pass(); passes++; }

// Least squares on ns = E*entries + T*instructions + C*calls, over the windows.
function solve3(rows) {
  // Normal equations, 3x3, solved by Gaussian elimination with partial pivoting.
  const A = [[0, 0, 0], [0, 0, 0], [0, 0, 0]];
  const b = [0, 0, 0];
  for (const r of rows) {
    const x = [r.n, r.instr, r.calls];
    for (let i = 0; i < 3; i++) { for (let j = 0; j < 3; j++) A[i][j] += x[i] * x[j]; b[i] += x[i] * r.y; }
  }
  const M = A.map((row, i) => [...row, b[i]]);
  for (let c = 0; c < 3; c++) {
    let p = c;
    for (let r = c + 1; r < 3; r++) if (Math.abs(M[r][c]) > Math.abs(M[p][c])) p = r;
    [M[c], M[p]] = [M[p], M[c]];
    for (let r = 0; r < 3; r++) {
      if (r === c) continue;
      const f = M[r][c] / M[c][c];
      for (let j = c; j < 4; j++) M[r][j] -= f * M[c][j];
    }
  }
  return [M[0][3] / M[0][0], M[1][3] / M[1][1], M[2][3] / M[2][2]];
}

const rows = windows.map((w) => ({ n: w.n, instr: w.instr, calls: w.calls, y: w.ns / passes }));
const [E, T, C] = solve3(rows);
// And the two-unknown fit, for readers who want the entry priced without the
// import term carrying any of it.
function solve2(rows) {
  let a = 0, b2 = 0, c = 0, d = 0, e = 0;
  for (const r of rows) { a += r.n * r.n; b2 += r.n * r.instr; c += r.instr * r.instr; d += r.n * r.y; e += r.instr * r.y; }
  const det = a * c - b2 * b2;
  return [(d * c - e * b2) / det, (a * e - b2 * d) / det];
}
const [E2, T2] = solve2(rows);

const totalNs = rows.reduce((s, r) => s + r.y, 0);
const perEntryRange = windows.map((w) => w.instr / w.n);
const engine = typeof Bun !== 'undefined' ? 'bun (JavaScriptCore)' : 'node (V8)';

console.log(JSON.stringify({
  engine,
  module: modulePath,
  moduleBytes: wasm.length,
  fnBlocks: meta.fnBlocks,
  windows: windows.length,
  windowEntries: W,
  passes,
  compileMs: Number(compileMs.toFixed(1)),
  entries: meta.entries,
  retired: meta.retired,
  importCalls: meta.calls,
  instrPerEntryMin: Number(Math.min(...perEntryRange).toFixed(2)),
  instrPerEntryMax: Number(Math.max(...perEntryRange).toFixed(2)),
  crossPerEntry: Number((Number(crossTotal) / meta.entries).toFixed(3)),
  nsPerEntryFlat: Number((totalNs / meta.entries).toFixed(1)),
  nsPerInstrFlat: Number((totalNs / meta.retired).toFixed(3)),
  fit3: { nsPerEntry: Number(E.toFixed(1)), nsPerInstr: Number(T.toFixed(3)), nsPerImportCall: Number(C.toFixed(1)) },
  fit2: { nsPerEntry: Number(E2.toFixed(1)), nsPerInstr: Number(T2.toFixed(3)) },
  loadavg: Number(loadavg()[0].toFixed(1)),
  identity: noRun ? 'NOT a replay — P6B_NO_RUN=1 times the harness alone' : 'checked',
}));
