// spike: the region-JIT replay, over HTTP, for mobile Safari.
//
// The browser twin of `tools/replay.mjs` and `examples/replay.rs`: same files,
// same loop, same "time only the `run` calls" rule, same identity checks. It
// fetches the recording instead of reading it off disk so a phone can run the
// exact module bytes the desk measured.
//
// Loaded by `replay.html`; also importable under bun/node for a sanity check.

const PAGES = 0x1006_0000 / 65536;
const SCRATCH = 0x1001_0000;
const REC = 316;

// One 256 MiB memory, shared by both recordings' instances. Allocating it
// twice on a phone is the difference between a result and an OOM.
let sharedMemory = null;

async function getBuf(base, path) {
  const r = await fetch(`${base}${path}`, { cache: "no-store" });
  if (!r.ok) throw new Error(`${path}: HTTP ${r.status}`);
  return new Uint8Array(await r.arrayBuffer());
}

/**
 * Replay one recording.
 * base: URL prefix ("" for same-origin, or "http://host:20966/")
 * dir:  "rec-rb" | "rec-rk"
 * reps: timed passes (the first pass is always the checked one)
 */
export async function replay(base, dir, reps, log = () => {}) {
  log(`${dir}: fetching`);
  const [wasm, snap, trace, mmio, delta] = await Promise.all([
    getBuf(base, `${dir}/region.wasm`),
    getBuf(base, `${dir}/mem.bin`),
    getBuf(base, `${dir}/trace.bin`),
    getBuf(base, `${dir}/mmio.bin`),
    getBuf(base, `${dir}/delta.bin`),
  ]);
  const n = trace.length / REC;
  if (!Number.isInteger(n)) throw new Error(`${dir}: trace.bin is not whole ${REC}-byte records`);

  log(`${dir}: allocating ${(PAGES * 65536) / (1 << 20)} MiB wasm memory`);
  if (!sharedMemory) sharedMemory = new WebAssembly.Memory({ initial: PAGES, maximum: PAGES });
  const memory = sharedMemory;
  const mem8 = new Uint8Array(memory.buffer);
  const memI = new Int32Array(memory.buffer);
  const memBI = new BigInt64Array(memory.buffer);

  // MMIO replay: hand back the recorded results in order. Both pinned regions
  // recorded zero of these (pure compute) but this stays exact anyway.
  const mmioVals =
    mmio.length > 0
      ? new BigInt64Array(mmio.buffer.slice(mmio.byteOffset, mmio.byteOffset + mmio.length))
      : new BigInt64Array(0);
  let mmioAt = 0;

  log(`${dir}: compiling ${wasm.length} bytes`);
  const tc0 = performance.now();
  const { instance } = await WebAssembly.instantiate(wasm, {
    env: {
      mem: memory,
      mmio_load: () => mmioVals[mmioAt++],
      mmio_store: () => Number(mmioVals[mmioAt++]),
    },
  });
  const compileMs = performance.now() - tc0;
  const run = instance.exports.run;

  // Decode the trace once into flat typed arrays so the timed loop parses
  // nothing. i64 arguments cross the JS/wasm boundary as BigInt; hoisting the
  // conversions out measures the region, not BigInt allocation.
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

  const sv = new DataView(snap.buffer, snap.byteOffset, snap.length);
  function loadSnapshot() {
    mem8.fill(0);
    let at = 0;
    while (at < snap.length) {
      const off = sv.getUint32(at, true);
      const len = sv.getUint32(at + 4, true);
      mem8.set(snap.subarray(at + 8, at + 8 + len), off);
      at += 8 + len;
    }
    mmioAt = 0;
  }

  const dd = new DataView(delta.buffer, delta.byteOffset, delta.length);
  const S4 = SCRATCH >> 2;
  const S8 = SCRATCH >> 3;

  function pass(check) {
    loadSnapshot();
    let mismatches = 0;
    let firstBad = -1;
    let instructions = 0;
    let ns = 0;
    let d = 0;
    for (let i = 0; i < n; i++) {
      // Stand in for the interpreter, which runs the blocks this region does
      // not cover, between entries, and writes memory. NOT timed.
      const count = dd.getUint32(d, true);
      d += 4;
      for (let c = 0; c < count; c++) {
        const off = dd.getUint32(d, true);
        const len = dd.getUint32(d + 4, true);
        mem8.set(delta.subarray(d + 8, d + 8 + len), off);
        d += 8 + len;
      }

      const rb = i * 32;
      for (let r = 0; r < 32; r++) memI[S4 + r] = regsIn[rb + r];
      memI[S4 + 35] = 0; // flag
      memBI[S8 + 18] = wlo[i]; // SCRATCH + 144
      memBI[S8 + 19] = whi[i]; // SCRATCH + 152

      const t = performance.now();
      const pc = run(idx[i], cyc0[i], endB[i]);
      ns += performance.now() - t;

      instructions += memI[S4 + 34];
      if (check) {
        let ok =
          pc === pcOut[i] &&
          memBI[S8 + 16] === cycOut[i] && // cycle count
          memI[S4 + 34] === ranOut[i] && // retired-instruction delta
          memI[S4 + 35] === flagOut[i]; // after-store flag
        if (ok) {
          for (let r = 1; r < 32; r++) {
            if (memI[S4 + r] !== regsOut[rb + r]) { ok = false; break; }
          }
        }
        if (!ok) { mismatches++; if (firstBad < 0) firstBad = i; }
      }
    }
    return { ms: ns, mismatches, firstBad, instructions };
  }

  log(`${dir}: checked pass`);
  const checked = pass(true);
  let best = Infinity;
  let instructions = checked.instructions;
  for (let p = 0; p < reps; p++) {
    log(`${dir}: timed pass ${p + 1}/${reps}`);
    const r = pass(false);
    best = Math.min(best, r.ms);
    instructions = r.instructions;
  }

  return {
    dir,
    entries: n,
    wasm_bytes: wasm.length,
    mem_bytes: snap.length,
    trace_bytes: trace.length,
    delta_bytes: delta.length,
    mmio_results: mmioVals.length,
    compile_ms: +compileMs.toFixed(2),
    identity_ok: checked.mismatches === 0,
    mismatches: checked.mismatches,
    first_bad_entry: checked.firstBad,
    instructions,
    instr_per_entry: +(instructions / n).toFixed(1),
    best_ms: +best.toFixed(2),
    ns_per_instruction: +((best * 1e6) / instructions).toFixed(3),
    m_instr_per_sec: +((instructions / best) * 1e-3).toFixed(1),
  };
}
