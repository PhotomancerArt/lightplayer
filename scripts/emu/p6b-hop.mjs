#!/usr/bin/env node
// M7 P6b (H2): what one cross-function hop costs, in isolation.
//
//   LP_EMU_JIT_ENGINE_CASE=$PWD/target/p6b/hop \
//     cargo test -p lp-emu-jit --features host-wasmtime --test translate_roundtrip \
//     a_ring_emitted
//   node scripts/emu/p6b-hop.mjs target/p6b/hop
//   bun  scripts/emu/p6b-hop.mjs target/p6b/hop
//
// The two cases the test writes are the SAME ring of 64 guest blocks, emitted
// once as a single sub-dispatcher and once one block to a function. The test
// has already asserted under wasmtime that they retire the same instructions
// for the same cycles and leave the same registers, so the only thing that
// differs is how many times control left one wasm function for another:
//
//   p6b-hop-whole   0 crosses
//   p6b-hop-split   one per block entered
//
// (wall(split) - wall(whole)) / crosses is therefore one hop, with no image,
// no host and no fit. Both cases make no import calls at all — the ring is
// `addi` and `jal` — so nothing about a JavaScript frame is in the number.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { loadavg } from 'node:os';

const dir = process.argv[2] ?? 'target/p6b/hop';
const reps = Number(process.argv[3] ?? 7);
// `narrow` blocks touch one guest register, `wide` blocks touch all 31. The
// epilogue stores and the prologue reloads exactly the chunk's live set, so
// the two rings price the hop's machinery and the hop a real image pays.
const rings = (process.argv[4] ?? 'narrow,wide').split(',');

const EXCHANGE_CYCLE = 128, EXCHANGE_INSTRET = 136, EXCHANGE_CROSS = 152;

function fnv1a(bytes) {
  let h = 0xcbf29ce484222325n;
  const mask = 0xffffffffffffffffn;
  for (let i = 0; i < bytes.length; i++) { h = (h ^ BigInt(bytes[i])) & mask; h = (h * 0x100000001b3n) & mask; }
  return h;
}

function load(name) {
  const spec = JSON.parse(readFileSync(join(dir, `${name}.json`), 'utf8'));
  const wasm = readFileSync(join(dir, `${name}.wasm`));
  const memory = new WebAssembly.Memory({ initial: spec.pages });
  const bytes = new Uint8Array(memory.buffer);
  const initial = Buffer.from(spec.initial, 'base64');
  if (fnv1a(initial) !== BigInt(spec.memoryFnv)) throw new Error(`${name}: the recorded memory does not hash to what it says`);
  const imports = { emu: {
    memory,
    mmio_load: () => { throw new Error(`${name}: the ring makes no MMIO access`); },
    mmio_store: () => { throw new Error(`${name}: the ring makes no MMIO access`); },
    step_one: () => { throw new Error(`${name}: the ring escapes nothing`); },
  } };
  const t0 = performance.now();
  const mod = new WebAssembly.Module(wasm);
  const compileMs = performance.now() - t0;
  const instance = new WebAssembly.Instance(mod, imports);
  return { spec, wasm, memory, bytes, initial, instance, compileMs, view: new DataView(memory.buffer) };
}

/** One `run`, from the recorded start state, timed and checked. */
function once(c, name) {
  c.bytes.set(c.initial);
  const t = performance.now();
  const pc = c.instance.exports.run(c.spec.entry, BigInt(c.spec.cycle), BigInt(c.spec.instret),
                                    BigInt(c.spec.end), BigInt(c.spec.watchLo), BigInt(c.spec.watchHi));
  const ms = performance.now() - t;
  const X = c.spec.exchange;
  const instret = c.view.getBigUint64(X + EXCHANGE_INSTRET, true);
  const cycle = c.view.getBigUint64(X + EXCHANGE_CYCLE, true);
  const cross = c.view.getBigUint64(X + EXCHANGE_CROSS, true);
  if ((pc >>> 0) !== c.spec.expect.pc) throw new Error(`${name}: left at ${(pc >>> 0).toString(16)}, expected ${c.spec.expect.pc.toString(16)}`);
  if (instret !== BigInt(c.spec.expect.instret) || cycle !== BigInt(c.spec.expect.cycle)) {
    throw new Error(`${name}: retired ${instret}/${cycle}, expected ${c.spec.expect.instret}/${c.spec.expect.cycle}`);
  }
  return { ms, instret, cycle, cross };
}

const engine = typeof Bun !== 'undefined' ? 'bun (JavaScriptCore)' : 'node (V8)';
for (const ring of rings) {
const out = {};
for (const name of [`p6b-hop-${ring}-whole`, `p6b-hop-${ring}-split`]) {
  const c = load(name);
  const runs = [];
  let last;
  for (let i = 0; i < reps; i++) { last = once(c, name); runs.push(last.ms); }
  runs.sort((a, b) => a - b);
  // The best of `reps` rather than the mean: this desk is shared, and the
  // fastest run is the one least contaminated by whatever else was on it.
  out[name] = {
    bestMs: Number(runs[0].toFixed(3)),
    medianMs: Number(runs[(reps / 2) | 0].toFixed(3)),
    instret: Number(last.instret),
    cross: Number(last.cross),
    compileMs: Number(c.compileMs.toFixed(1)),
    moduleBytes: c.wasm.length,
    functions: null,
  };
}

const a = out[`p6b-hop-${ring}-whole`], b = out[`p6b-hop-${ring}-split`];
if (a.instret !== b.instret) throw new Error('the two cases did not retire the same instructions');
const dCross = b.cross - a.cross;
console.log(JSON.stringify({
  engine,
  ring,
  instructionsRetired: a.instret,
  whole: a,
  split: b,
  crosses: dCross,
  nsPerInstrWhole: Number((a.bestMs * 1e6 / a.instret).toFixed(3)),
  nsPerInstrSplit: Number((b.bestMs * 1e6 / b.instret).toFixed(3)),
  nsPerHopBest: Number(((b.bestMs - a.bestMs) * 1e6 / dCross).toFixed(2)),
  nsPerHopMedian: Number(((b.medianMs - a.medianMs) * 1e6 / dCross).toFixed(2)),
  loadavg: Number(loadavg()[0].toFixed(1)),
}, null, 0));
}
