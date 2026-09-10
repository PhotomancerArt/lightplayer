// spike: replay a recorded region trace in a browser-family engine.
//
//   bun  replay.mjs rec-rk     # JavaScriptCore — the iPhone's engine family
//   node replay.mjs rec-rk     # V8, a labelled second point
//
// Pass 1 CHECKS every architectural output against the native recording (this
// is the identity check in the other engine). Passes 2..n reload the memory
// snapshot and re-run untimed-checks-off for the throughput number; the best
// pass is reported, so the engine's tiering has settled.
import { readFileSync } from "node:fs";

const dir = process.argv[2] ?? "rec-rk";
const PAGES = 0x1006_0000 / 65536;
const SCRATCH = 0x1001_0000;
const REC = 316;
const TIMED_PASSES = 4;

const wasm = readFileSync(`${dir}/region.wasm`);
const snap = readFileSync(`${dir}/mem.bin`);
const trace = readFileSync(`${dir}/trace.bin`);
const mmio = readFileSync(`${dir}/mmio.bin`);
// The region is not self-contained: the interpreter runs the blocks the region
// does not cover, between entries, and writes memory. `delta.bin` carries the
// granules that changed since the previous entry, so the replay can stand in
// for the interpreter. Applying it is NOT counted in the throughput number —
// only the `run` calls are.
const delta = readFileSync(`${dir}/delta.bin`);
const n = trace.length / REC;
if (!Number.isInteger(n)) throw new Error(`trace.bin is not a whole number of ${REC}-byte records`);

const memory = new WebAssembly.Memory({ initial: PAGES, maximum: PAGES });
const mem8 = new Uint8Array(memory.buffer);
const memI = new Int32Array(memory.buffer);
const memBI = new BigInt64Array(memory.buffer);

// MMIO replay: hand back the recorded results in order. (Both pinned regions
// recorded zero of these — they are pure compute — but the harness is exact
// either way.)
const mmioVals = new BigInt64Array(mmio.buffer, mmio.byteOffset, mmio.length / 8);
let mmioAt = 0;
const imports = {
  env: {
    mem: memory,
    mmio_load: () => mmioVals[mmioAt++],
    mmio_store: () => Number(mmioVals[mmioAt++]),
  },
};

function loadSnapshot() {
  mem8.fill(0);
  const dv = new DataView(snap.buffer, snap.byteOffset, snap.length);
  let at = 0;
  while (at < snap.length) {
    const off = dv.getUint32(at, true);
    const len = dv.getUint32(at + 4, true);
    mem8.set(snap.subarray(at + 8, at + 8 + len), off);
    at += 8 + len;
  }
  mmioAt = 0;
}

// Decode the trace once, into flat typed arrays, so the timed loop does no
// parsing. i64 arguments cross the JS/wasm boundary as BigInt; hoisting the
// conversions out of the loop measures the region itself rather than BigInt
// allocation. The in-loop cost is reported separately.
const tv = new DataView(trace.buffer, trace.byteOffset, trace.length);
const idx = new Int32Array(n);
const cyc0 = new BigInt64Array(n);
const endB = new BigInt64Array(n);
const wlo = new BigInt64Array(n);
const whi = new BigInt64Array(n);
const regsIn = new Int32Array(n * 32);
const pcOut = new Int32Array(n);
const cycOut = new BigInt64Array(n);
const ranOut = new Int32Array(n);
const flagOut = new Int32Array(n);
const regsOut = new Int32Array(n * 32);
for (let i = 0; i < n; i++) {
  const b = i * REC;
  idx[i] = tv.getInt32(b, true);
  cyc0[i] = tv.getBigInt64(b + 4, true);
  endB[i] = tv.getBigInt64(b + 12, true);
  wlo[i] = tv.getBigInt64(b + 20, true);
  whi[i] = tv.getBigInt64(b + 28, true);
  for (let r = 0; r < 32; r++) regsIn[i * 32 + r] = tv.getInt32(b + 36 + 4 * r, true);
  pcOut[i] = tv.getInt32(b + 168, true);
  cycOut[i] = tv.getBigInt64(b + 172, true);
  ranOut[i] = tv.getInt32(b + 180, true);
  flagOut[i] = tv.getInt32(b + 184, true);
  for (let r = 0; r < 32; r++) regsOut[i * 32 + r] = tv.getInt32(b + 188 + 4 * r, true);
}

const S4 = SCRATCH >> 2;
const S8 = SCRATCH >> 3;

const t0 = performance.now();
const module = new WebAssembly.Module(wasm);
const t1 = performance.now();
const instance = new WebAssembly.Instance(module, imports);
const run = instance.exports.run;
const t2 = performance.now();

const dd = new DataView(delta.buffer, delta.byteOffset, delta.length);

function pass(check) {
  loadSnapshot();
  let mismatches = 0;
  let instructions = 0;
  let ns = 0;
  let d = 0;
  for (let i = 0; i < n; i++) {
    // Stand in for the interpreter's writes between entries.
    const count = dd.getUint32(d, true);
    d += 4;
    for (let c = 0; c < count; c++) {
      const off = dd.getUint32(d, true);
      const len = dd.getUint32(d + 4, true);
      mem8.set(delta.subarray(d + 8, d + 8 + len), off);
      d += 8 + len;
    }

    const base = i * 32;
    for (let r = 0; r < 32; r++) memI[S4 + r] = regsIn[base + r];
    memI[S4 + 35] = 0; // flag
    memBI[S8 + 18] = wlo[i]; // SCRATCH + 144
    memBI[S8 + 19] = whi[i]; // SCRATCH + 152
    const t = performance.now();
    const pc = run(idx[i], cyc0[i], endB[i]);
    ns += performance.now() - t;
    instructions += memI[S4 + 34];
    if (check) {
      if (pc !== pcOut[i]) mismatches++;
      else if (memBI[S8 + 16] !== cycOut[i]) mismatches++;
      else if (memI[S4 + 34] !== ranOut[i]) mismatches++;
      else if (memI[S4 + 35] !== flagOut[i]) mismatches++;
      else {
        for (let r = 1; r < 32; r++) {
          if (memI[S4 + r] !== regsOut[base + r]) {
            mismatches++;
            break;
          }
        }
      }
    }
  }
  return { ms: ns, mismatches, instructions };
}

const checked = pass(true);
let best = Infinity;
let instr = checked.instructions;
for (let p = 0; p < TIMED_PASSES; p++) {
  const r = pass(false);
  best = Math.min(best, r.ms);
  instr = r.instructions;
}

const engine = typeof Bun !== "undefined" ? `bun ${Bun.version} (JavaScriptCore)` : `node ${process.version} (V8)`;
console.log(`engine            ${engine}`);
console.log(`dir               ${dir}  (${wasm.length} wasm bytes, ${n} entries)`);
console.log(`compile           ${(t1 - t0).toFixed(1)} ms`);
console.log(`instantiate       ${(t2 - t1).toFixed(1)} ms`);
console.log(`identity          ${checked.mismatches === 0 ? "OK — every entry matched the native recording" : `${checked.mismatches} MISMATCHES of ${n}`}`);
console.log(`instructions      ${instr} (${(instr / n).toFixed(1)} per entry)`);
console.log(`best timed pass   ${best.toFixed(1)} ms`);
console.log(`throughput        ${((instr / best) * 1e-3).toFixed(1)} M instr/s   (${((best * 1e6) / instr).toFixed(2)} ns/instruction)`);
