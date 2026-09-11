#!/usr/bin/env node
// M7 P6c (Q4): what a **module-side entry** is made of, in JavaScriptCore and
// in V8.
//
//   bun  scripts/emu/p6c-jsc-entry.mjs [--seconds N]
//   node scripts/emu/p6c-jsc-entry.mjs [--seconds N]
//
// P6b priced the module side of an entry at **39 ns in V8 and 326 ns in
// bun/JSC** by least-squares over windows of a real recording. That is a
// total, and a total cannot be acted on: 326 ns could be the JS→wasm call
// boundary, the selector's own prologue, the locals wasm zeroes at entry, or
// the 31-register reload the sub-dispatcher does. The phone is JSC-family, so
// which of the four it is decides whether M7b has a lever there at all.
//
// So: a ladder of five modules that are the same module plus one thing each,
// every one of them exporting the **real selector's signature** —
// `(i32, i64, i64, i64, i64, i64) -> i32` — and called in a tight JS loop with
// the real argument shapes. The difference between two rungs is the thing that
// was added and can be nothing else.
//
//   A  bare        return a constant. The JS→wasm call boundary, nothing else.
//   B  locals      + the selector's own six local slots (JD17's `next`, the
//                  sub-dispatcher's result, fidx, exit pc, cycle, instret).
//   C  prologue    + the selector's exchange traffic: three i32/i64 stores
//                  (flags, cross, indirect-miss) and two i64 loads (cycle,
//                  instret) at the exchange area, plus the fidx divide.
//   D  dispatch    + one `call_indirect` through a function table to a
//                  sub-dispatcher-shaped body that declares the emitted
//                  code's **46 local slots** (43 i32, 3 i64 — P6b H1's
//                  measured, size-independent number) and returns.
//   E  registers   + that body reloads 31 guest registers from the exchange
//                  area into its locals and writes them back, which is what
//                  a real sub-dispatcher's prologue and epilogue do.
//
// Rung E is a whole entry into a sub-dispatcher that immediately leaves, so
// `E` is the floor under every real entry: no guest instruction has retired
// yet. Everything above it in a real run is guest work.
//
// Each rung is timed in the same invocation, interleaved and repeated, and the
// **best** of the repeats is reported — a shared desk can only make a rung
// slower (the rule `p6b-rows.mjs` states).
import { loadavg } from 'node:os';

const args = process.argv.slice(2);
let seconds = 1.5;
let reps = 5;
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--seconds') seconds = Number(args[++i]);
  else if (args[i] === '--reps') reps = Number(args[++i]);
  else throw new Error('unknown option ' + args[i]);
}

// --- the smallest wasm encoder that can write these five modules -----------
const I32 = 0x7f, I64 = 0x7e, FUNCREF = 0x70;

function uleb(n) {
  const out = [];
  do { let b = n & 0x7f; n >>>= 7; if (n) b |= 0x80; out.push(b); } while (n);
  return out;
}
function sleb(n) {
  const out = [];
  let more = true;
  while (more) {
    let b = n & 0x7f;
    n >>= 7;
    if ((n === 0 && !(b & 0x40)) || (n === -1 && (b & 0x40))) more = false; else b |= 0x80;
    out.push(b);
  }
  return out;
}
function vec(items) { return [...uleb(items.length), ...items.flat()]; }
function section(id, body) { return [id, ...uleb(body.length), ...body]; }

// The selector's signature, and the sub-dispatcher's.
const STAY = [I32, I64, I64, I64, I64, I64];
const TYPE_SELECTOR = [0x60, ...vec(STAY.map((t) => [t])), ...vec([[I32]])];
const TYPE_SUB = [0x60, ...vec(STAY.map((t) => [t])), ...vec([[I64]])];

const EXCHANGE = 1024; // where the fake exchange area lives in the module's memory

/// Build one rung. `rung` is 0..4 for A..E.
function build(rung) {
  const types = [TYPE_SELECTOR, TYPE_SUB];
  const hasSub = rung >= 3;

  // --- the exported selector's body
  const code = [];
  const e = (...b) => code.push(...b);
  // locals, from rung B on: the selector's own six slots.
  const locals = rung >= 1
    ? vec([[...uleb(4), I32], [...uleb(2), I64]])   // 4 i32, 2 i64
    : vec([]);

  if (rung >= 2) {
    // the exchange prologue: three stores and two loads, the shapes the real
    // selector uses (i32.store for flags, i64.store for the two counters).
    e(0x41, ...sleb(0), 0x41, ...sleb(0), 0x36, 0x02, ...uleb(EXCHANGE));        // i32.store flags
    e(0x41, ...sleb(0), 0x42, ...sleb(0), 0x37, 0x03, ...uleb(EXCHANGE + 8));    // i64.store cross
    e(0x41, ...sleb(0), 0x42, ...sleb(0), 0x37, 0x03, ...uleb(EXCHANGE + 16));   // i64.store indirect-miss
    // fidx = entry >> 6, into local 6 (the first declared local after 6 params)
    e(0x20, ...uleb(0), 0x41, ...sleb(6), 0x76, 0x21, ...uleb(6));               // local.set fidx
    // read the two counters back
    e(0x41, ...sleb(0), 0x29, 0x03, ...uleb(EXCHANGE + 24), 0x21, ...uleb(10));  // i64.load -> cycle
    e(0x41, ...sleb(0), 0x29, 0x03, ...uleb(EXCHANGE + 32), 0x21, ...uleb(11));  // i64.load -> instret
  }

  if (hasSub) {
    // the six stay arguments, then the table index, then call_indirect.
    for (let p = 0; p < 6; p++) e(0x20, ...uleb(p));
    e(0x41, ...sleb(0));                                   // table index 0
    e(0x11, ...uleb(1), 0x00);                             // call_indirect type 1, table 0
    e(0xa7);                                               // i32.wrap_i64
    e(0x0f);                                               // return
  } else {
    // One memory load, in every rung including the bare one, so that no rung
    // is a constant an engine can fold or inline away: V8 does inline small
    // wasm bodies into optimized JS, and a rung that measures an inlined
    // constant is not measuring a call.
    e(0x41, ...sleb(0), 0x28, 0x02, ...uleb(EXCHANGE));
    e(0x0f);
  }
  e(0x0b);                                                 // end

  const selectorBody = [...locals, ...code];

  // --- the sub-dispatcher's body: 46 locals, and at rung E the register traffic
  const subCode = [];
  const s = (...b) => subCode.push(...b);
  if (rung >= 4) {
    // 31 loads from the exchange area into locals, then 31 stores back —
    // exactly what a sub-dispatcher's prologue and epilogue move.
    for (let r = 0; r < 31; r++) {
      s(0x41, ...sleb(0), 0x28, 0x02, ...uleb(EXCHANGE + 64 + 4 * r));  // i32.load
      s(0x21, ...uleb(6 + r));                                          // local.set
    }
    for (let r = 0; r < 31; r++) {
      s(0x41, ...sleb(0), 0x20, ...uleb(6 + r), 0x36, 0x02, ...uleb(EXCHANGE + 64 + 4 * r));
    }
  }
  s(0x42, ...sleb(0));  // i64.const 0
  s(0x0b);
  // P6b H1: 46 local slots in 6 groups, 43 i32 and 3 i64, at every size.
  const subLocals = vec([[...uleb(43), I32], [...uleb(3), I64]]);
  const subBody = [...subLocals, ...subCode];

  const funcs = hasSub ? [[0], [1]] : [[0]];
  const bodies = hasSub
    ? [[...uleb(selectorBody.length), ...selectorBody], [...uleb(subBody.length), ...subBody]]
    : [[...uleb(selectorBody.length), ...selectorBody]];

  const mod = [
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
    ...section(1, vec(types.map((t) => t))),
    ...section(3, vec(funcs)),
    ...(hasSub ? section(4, vec([[FUNCREF, 0x01, ...uleb(1), ...uleb(1)]])) : []),
    ...section(5, vec([[0x00, ...uleb(1)]])),                      // one memory page
    ...section(7, vec([[...uleb(3), 0x72, 0x75, 0x6e, 0x00, ...uleb(0)]])), // export "run" = func 0
    ...(hasSub
      ? section(9, vec([[0x00, 0x41, ...sleb(0), 0x0b, ...vec([[...uleb(1)]])]]))
      : []),
    ...section(10, vec(bodies)),
  ];
  return new Uint8Array(mod);
}

const RUNGS = [
  ['A  bare (the JS→wasm call boundary)', 'the call itself'],
  ['B  + the selector’s 6 locals', 'locals'],
  ['C  + the exchange prologue', 'the prologue'],
  ['D  + call_indirect into a 46-local body', 'dispatch + 46 locals'],
  ['E  + the 31-register reload and write-back', 'the register traffic'],
];

const engine = typeof Bun !== 'undefined' ? `bun/JavaScriptCore ${Bun.version}` : `node/V8 ${process.versions.node}`;
console.log(`engine     ${engine}`);
console.log(`ladder     ${reps} rep(s) of ${seconds}s per rung, interleaved, best-of reported`);
console.log('');

const built = RUNGS.map((_, i) => {
  const bytes = build(i);
  const mod = new WebAssembly.Module(bytes);
  const inst = new WebAssembly.Instance(mod, {});
  return { bytes, run: inst.exports.run };
});

function time(run, budgetMs) {
  // Warm first, so a tiering engine is measured at steady state and not
  // during its own tier-up — the same reason `jit-image-bench.mjs` reports a
  // first-second rate separately.
  for (let i = 0; i < 200_000; i++) run(i & 63, 0n, 0n, 1n, 0n, 0n);
  let n = 0;
  const t0 = performance.now();
  let t1 = t0;
  const CHUNK = 100_000;
  do {
    for (let i = 0; i < CHUNK; i++) run(i & 63, 0n, 0n, 1n, 0n, 0n);
    n += CHUNK;
    t1 = performance.now();
  } while (t1 - t0 < budgetMs);
  return ((t1 - t0) * 1e6) / n;
}

const best = RUNGS.map(() => Infinity);
for (let rep = 0; rep < reps; rep++) {
  for (let i = 0; i < RUNGS.length; i++) {
    const ns = time(built[i].run, seconds * 1000);
    if (ns < best[i]) best[i] = ns;
  }
}

console.log('rung'.padEnd(42) + 'ns/call'.padStart(10) + 'delta'.padStart(10) + '  what the delta is');
console.log('-'.repeat(96));
for (let i = 0; i < RUNGS.length; i++) {
  const d = i === 0 ? best[i] : best[i] - best[i - 1];
  console.log(
    RUNGS[i][0].padEnd(42) +
    best[i].toFixed(2).padStart(10) +
    d.toFixed(2).padStart(10) +
    '  ' + RUNGS[i][1],
  );
}
console.log('-'.repeat(96));
console.log(`module bytes: ${built.map((b) => b.bytes.length).join(' / ')}`);
console.log(`loadavg ${loadavg()[0].toFixed(1)}`);
