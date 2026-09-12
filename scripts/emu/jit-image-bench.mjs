#!/usr/bin/env node
// A whole-image translated module, run in a real wasm engine against a real
// recording — the measurement M7 JD26 sizes two-level dispatch on.
//
//   node scripts/emu/jit-image-bench.mjs <recording-dir> <module.wasm> [seconds]
//   bun  scripts/emu/jit-image-bench.mjs <recording-dir> <module.wasm> [seconds]
//   node scripts/emu/jit-image-bench.mjs <recording-dir> <module.wasm> --check-only
//
// `--check-only` is the identity pass alone — one walk of the recording with
// every field compared, no stopwatch. `just test-emu-jit-identity` runs it.
//
// `node` is V8 and `bun` is JavaScriptCore, which is the phone's engine
// family. JD19: a single-engine wasm number is not a wasm number.
//
// # Why a recording and not a host
//
// The module imports `mmio_load`, `mmio_store` and `step_one`, which are a
// bus, a peripheral model and an interpreter. Writing those in JavaScript
// would be a second implementation of the machine and a divergence waiting to
// happen — the same reason `jit-engine-check.mjs` replays answers instead of
// producing them. So a native `--jit` run records what each entry into
// translated code was handed and what every import answered, in call order,
// and this hands the same answers back. See `lp-emu-esp32c6/src/jit_record.rs`.
//
// Because the recording also carries what each entry *produced* — the exit pc,
// the flags, both counters and x1..x31 — a replay is an **identity check in
// this engine** as well as a stopwatch, and it fails loudly on the first
// divergence rather than reporting a fast wrong number.
//
// # Why one recording drives every size
//
// A global block index does not depend on how the module is split: the block
// set is the same and the exchange protocol is the same. So the recording is
// taken once, under cranelift, and every `--jit-fn-blocks` size is emitted
// with `--jit-emit-only` and replayed against it. Cranelift is off the sizing
// loop entirely, which is what makes the table affordable.
//
// # What it reports
//
// Compile ms, instantiate ms, and **ns per guest instruction** at steady state
// beside the first second's rate, so a lazily tiering engine's tier-up cost is
// visible rather than averaged away.

import { readFileSync } from "node:fs";
import { join } from "node:path";

const argv = process.argv.slice(2);
// `--check-only` is the identity half without the stopwatch (M7 P9): one full
// pass over the recording, every field compared, then stop. That is what
// `just test-emu-jit-identity` runs, and it is deliberately NOT the same call
// as a bench row.
//
// It exists because the timing loop replays the recording over and over, and
// a SECOND pass is not a replay of the same thing: the module's published-read
// block (M7b P3) is refreshed inside `mmio_store`'s own crossing, which canned
// answers do not perform, so from the second iteration on the module can ask
// for a read the recording never recorded. The first pass is unaffected and is
// the whole of the identity claim. See the crate README, "What the replay
// harness was getting wrong (P5)", and follow-up F5.
const checkOnly = argv.includes("--check-only");
const positional = argv.filter((a) => !a.startsWith("--"));
const dir = positional[0];
const modulePath = positional[1];
const seconds = Number(positional[2] ?? 6);
if (!dir || !modulePath) {
  console.error("usage: jit-image-bench.mjs <recording-dir> <module.wasm> [seconds] [--check-only]");
  process.exit(2);
}

const meta = JSON.parse(readFileSync(join(dir, "meta.json"), "utf8"));
const wasm = readFileSync(modulePath);

// ---- the recording --------------------------------------------------------

/** The live ranges the replay puts back between iterations. */
function readImage() {
  const buf = readFileSync(join(dir, "memory.bin"));
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  const out = [];
  let at = 0;
  while (at < buf.byteLength) {
    const offset = view.getUint32(at, true);
    const len = view.getUint32(at + 4, true);
    out.push({ offset, bytes: buf.subarray(at + 8, at + 8 + len) });
    at += 8 + len;
  }
  return out;
}

const CALL_BYTES = 40;

function readEntries() {
  const buf = readFileSync(join(dir, "entries.bin"));
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  const out = [];
  let at = 0;
  const u32 = () => {
    const v = view.getUint32(at, true);
    at += 4;
    return v;
  };
  const i32 = () => {
    const v = view.getInt32(at, true);
    at += 4;
    return v;
  };
  const u64 = () => {
    const v = view.getBigUint64(at, true);
    at += 8;
    return v;
  };
  while (at < buf.byteLength) {
    const entry = u32();
    const cycleIn = u64();
    const instretIn = u64();
    const end = u64();
    const watchLo = u64();
    const watchHi = u64();
    // The interpreter runs between entries and writes registers, and
    // registers are not memory, so the delta cannot carry them.
    const regsIn = new Int32Array(31);
    for (let i = 0; i < 31; i++) regsIn[i] = i32();
    const delta = [];
    for (let n = u32(); n > 0; n--) {
      const offset = u32();
      const len = u32();
      delta.push({ offset, bytes: buf.subarray(at, at + len) });
      at += len;
    }
    const callCount = u32();
    // Kept as a flat view rather than objects: a 20 ms window is a quarter of
    // a million calls, and a quarter of a million objects is the benchmark.
    const calls = new DataView(buf.buffer, buf.byteOffset + at, callCount * CALL_BYTES);
    at += callCount * CALL_BYTES;
    const exitPc = u32();
    const flags = i32();
    const cycleOut = u64();
    const instretOut = u64();
    const regs = new Int32Array(31);
    for (let i = 0; i < 31; i++) regs[i] = i32();
    out.push({
      entry,
      cycleIn,
      instretIn,
      end,
      watchLo,
      watchHi,
      regsIn,
      delta,
      calls,
      callCount,
      exitPc,
      flags,
      cycleOut,
      instretOut,
      regs,
    });
  }
  return out;
}

const image = readImage();
const entries = readEntries();

// ---- the exchange area, as `lp-emu-jit`'s `host` module lays it out --------

const EXCHANGE_REGS = 0;
const EXCHANGE_CYCLE = 128;
const EXCHANGE_INSTRET = 136;
const EXCHANGE_FLAGS = 144;
const EXCHANGE_CROSS = 152;
const X = meta.exchange;

const memory = new WebAssembly.Memory({ initial: meta.pages });
const bytes = new Uint8Array(memory.buffer);
const view = new DataView(memory.buffer);
// The register file as a typed array, so an entry's 31 inputs are one `set`
// rather than 31 `setInt32`s. The replay pays a JS frame per entry whatever
// happens — see the note on the rate below — and everything else in that
// frame is worth deleting.
const regFile = new Int32Array(memory.buffer, X + EXCHANGE_REGS, 32);

function restore() {
  for (const r of image) bytes.set(r.bytes, r.offset);
}

// ---- the imports, replayed ------------------------------------------------

let current = null;
let cursor = 0;

function nextCall(kind, pc, address) {
  if (cursor >= current.callCount) {
    throw new Error(
      `entry ${current.entry}: the module made more import calls than the recording has`,
    );
  }
  const at = cursor * CALL_BYTES;
  cursor++;
  const c = current.calls;
  if (c.getUint8(at) !== kind) {
    throw new Error(
      `entry ${current.entry}, call ${cursor - 1}: the module asked for kind ${kind} where ` +
        `the recording has ${c.getUint8(at)}`,
    );
  }
  if (c.getUint32(at + 4, true) !== pc >>> 0 || c.getUint32(at + 16, true) !== address >>> 0) {
    throw new Error(
      `entry ${current.entry}, call ${cursor - 1}: the module asked at pc ` +
        `${(pc >>> 0).toString(16)} address ${(address >>> 0).toString(16)}, the recording has ` +
        `${c.getUint32(at + 4, true).toString(16)} / ${c.getUint32(at + 16, true).toString(16)}`,
    );
  }
  return at;
}

const imports = {
  emu: {
    memory,
    mmio_load(pc, _cycle, address, _kind) {
      return current.calls.getBigUint64(nextCall(0, pc, address) + 32, true);
    },
    // `(status << 32) | pc`, an **i64** since M7b P2 gave the store its own
    // status word and its three post-store polling-point arguments. It used
    // to answer a bare `i32` pc and this harness still masked it down to one,
    // which V8 answers with `TypeError: Cannot convert <pc> to a BigInt` from
    // inside the module. The recording holds the whole 64-bit answer.
    mmio_store(pc, _cycle, address, _kind, _value) {
      return current.calls.getBigUint64(nextCall(1, pc, address) + 32, true);
    },
    // M7b P2's fourth import. A poll is recorded with `address` zero
    // (`jit.rs`'s `CallRec { kind: 2, address: 0, .. }`) and answers
    // `(status << 32) | pc`, exactly as a store does — both of them answer a
    // polling point. Without this the module does not even instantiate:
    // `LinkError: Import #3 "emu" "poll": function import requires a
    // callable`, which is what this harness did on `main` from #713 until
    // M7b P5 found it.
    poll(pc, _cycle, _instret) {
      return current.calls.getBigUint64(nextCall(2, pc, 0) + 32, true);
    },
    step_one(pc) {
      throw new Error(
        `the escape hatch fired at pc ${(pc >>> 0).toString(16)}; the recorder refuses to ` +
          "record that case, so this recording cannot contain it",
      );
    },
  },
};

// ---- compile, instantiate -------------------------------------------------

const t0 = performance.now();
const mod = new WebAssembly.Module(wasm);
const compileMs = performance.now() - t0;

const t1 = performance.now();
const instance = new WebAssembly.Instance(mod, imports);
const instantiateMs = performance.now() - t1;

const run = instance.exports.run;

// ---- one iteration of the whole recording ---------------------------------

let crosses = 0n;

// M7b P5: the harness's own per-iteration cost, measured rather than assumed.
//
// A timed iteration is not only the module: it is also the between-entries
// memory delta, the 31-register write and four `DataView` stores, PER ENTRY.
// This file's own module docs used to claim the delta was outside what a
// replay times. It never was — it is the first thing inside the loop
// `measure()` brackets — and on a boot recording that did not matter
// (256 bytes an entry) while on a render-loop one it is 69,120 bytes an entry
// and would swamp the number outright.
//
// So the same loop is run once with `run` skipped, and the difference is what
// the module cost. `steadyNsPerInstrNet` is the corrected rate and is the one
// a residual should be read off; `steadyNsPerInstr` stays as it was so older
// rows remain comparable with themselves.
function iteration(check, noRun = false) {
  let retired = 0n;
  for (const e of entries) {
    for (const d of e.delta) bytes.set(d.bytes, d.offset);
    regFile.set(e.regsIn, 1);
    regFile[0] = 0;
    view.setBigUint64(X + EXCHANGE_CYCLE, e.cycleIn, true);
    view.setBigUint64(X + EXCHANGE_INSTRET, e.instretIn, true);
    current = e;
    cursor = 0;
    if (noRun) {
      retired += e.instretOut - e.instretIn;
      continue;
    }
    const pc = run(e.entry, e.cycleIn, e.instretIn, e.end, e.watchLo, e.watchHi);
    if (check) {
      if ((pc >>> 0) !== e.exitPc) {
        throw new Error(
          `entry ${e.entry}: left at ${(pc >>> 0).toString(16)}, the recording says ` +
            `${e.exitPc.toString(16)}`,
        );
      }
      const cycle = view.getBigUint64(X + EXCHANGE_CYCLE, true);
      const instret = view.getBigUint64(X + EXCHANGE_INSTRET, true);
      if (cycle !== e.cycleOut || instret !== e.instretOut) {
        throw new Error(
          `entry ${e.entry}: charged ${cycle}/${instret}, the recording says ` +
            `${e.cycleOut}/${e.instretOut}`,
        );
      }
      const flags = view.getInt32(X + EXCHANGE_FLAGS, true);
      // Bit 2 is the pending-yield obligation a sub-dispatcher hands the next
      // one; the host does not read it and the recording does not carry it.
      if ((flags & 3) !== (e.flags & 3)) {
        throw new Error(`entry ${e.entry}: flags ${flags & 3}, the recording says ${e.flags & 3}`);
      }
      for (let i = 0; i < 31; i++) {
        const got = regFile[i + 1];
        if (got !== e.regs[i]) {
          throw new Error(
            `entry ${e.entry}: x${i + 1} is ${got}, the recording says ${e.regs[i]}`,
          );
        }
      }
      if (cursor !== e.callCount) {
        throw new Error(
          `entry ${e.entry}: the module made ${cursor} import calls, the recording has ` +
            `${e.callCount}`,
        );
      }
    }
    // Counted here rather than assumed: a cross-function edge flushes and
    // reloads the whole live register set through the exchange area, so its
    // rate is what a blocks-per-function size is really buying or spending.
    crosses += view.getBigUint64(X + EXCHANGE_CROSS, true);
    retired += e.instretOut - e.instretIn;
  }
  return retired;
}

// The first iteration is the identity check, and it is checked in full: every
// exit pc, both counters, the flags the host reads, all 31 registers and the
// import call count. A fast wrong number is worse than no number.
restore();
crosses = 0n;
const checked = iteration(true);
const crossesPerIteration = crosses;
if (checked !== BigInt(meta.retired)) {
  throw new Error(`the recording says ${meta.retired} instructions, the replay retired ${checked}`);
}

// ---- the rate, first second against steady state --------------------------

function measure(budgetMs, noRun = false) {
  let ns = 0;
  let instructions = 0n;
  let iterations = 0;
  const started = performance.now();
  while (performance.now() - started < budgetMs) {
    restore();
    const t = performance.now();
    instructions += iteration(false, noRun);
    ns += (performance.now() - t) * 1e6;
    iterations++;
  }
  return { nsPerInstr: ns / Number(instructions), iterations };
}

if (checkOnly) {
  const engine = typeof Bun !== "undefined" ? "bun (JavaScriptCore)" : "node (V8)";
  console.log(
    `${engine}: identity OK — ${meta.entries} entries, ${meta.retired} instructions, ` +
      `exit pc + cycle + instret + flags + x1..x31 + import call count on every one ` +
      `(${modulePath}, ${wasm.length} B, ${meta.fnBlocks} blocks/fn)`,
  );
  process.exit(0);
}

const first = measure(1000);
const steady = measure(seconds * 1000);
// The floor, taken last and at the same tier the steady measurement reached,
// so it is the same JavaScript running the same shapes with one call removed.
const floor = measure(1000, true);

const engine = typeof Bun !== "undefined" ? "bun (JavaScriptCore)" : "node (V8)";
console.log(
  JSON.stringify({
    engine,
    module: modulePath,
    moduleBytes: wasm.length,
    fnBlocks: meta.fnBlocks,
    blocks: meta.blocks,
    entries: meta.entries,
    recordedRetired: meta.retired,
    compileMs: Number(compileMs.toFixed(1)),
    instantiateMs: Number(instantiateMs.toFixed(2)),
    firstSecondNsPerInstr: Number(first.nsPerInstr.toFixed(3)),
    steadyNsPerInstr: Number(steady.nsPerInstr.toFixed(3)),
    harnessNsPerInstr: Number(floor.nsPerInstr.toFixed(3)),
    steadyNsPerInstrNet: Number((steady.nsPerInstr - floor.nsPerInstr).toFixed(3)),
    deltaBytesPerEntry: meta.deltaBytes,
    instrPerEntry: Number((meta.retired / meta.entries).toFixed(2)),
    crossesPerIteration: Number(crossesPerIteration),
    crossPerThousandInstr: Number(
      ((crossesPerIteration * 1000n) / BigInt(meta.retired)).toString(),
    ),
    firstSecondIterations: first.iterations,
    steadyIterations: steady.iterations,
    identity: "checked",
  }),
);
