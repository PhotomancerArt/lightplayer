#!/usr/bin/env node
// The translated module's answers, checked in a second and a third engine.
//
//   node scripts/emu/jit-engine-check.mjs target/jit-cases      # V8
//   bun  scripts/emu/jit-engine-check.mjs target/jit-cases      # JavaScriptCore
//   node scripts/emu/jit-engine-check.mjs target/xt-jit-cases   # the Xtensa cases
//
// The cases come from the round-trip tests, which have already asserted them
// under wasmtime:
//
//   LP_EMU_JIT_ENGINE_CASE=$PWD/target/jit-cases \
//     cargo test -p lp-emu-jit --features host-wasmtime --test translate_roundtrip
//   LP_EMU_XT_JIT_ENGINE_CASE=$PWD/target/xt-jit-cases \
//     cargo test -p lp-xt-jit --features host-wasmtime
//
// and, at image scale, from the classic's `LP_EMU_XT_JIT_RECORD=<dir>` run.
//
// Each case carries the module bytes, the linear memory it starts from, the
// answers its imports gave in call order, and everything the run produced. So
// this script needs no host: it replays the recorded answers and compares. That
// is the point — **the same module bytes** run in all three engines against the
// same inputs, so a divergence is the engine or the emitter and cannot be a
// second implementation of the host disagreeing with the first.
//
// M7 JD19: a single-engine wasm answer is not a wasm answer. `bun` is
// JavaScriptCore, which is the phone's engine family; `node` is V8. The spike
// found V8 and JSC disagreeing by 1.6x on speed, and P2 found JSC's fused
// `Table.grow` silently producing a table `call_indirect` could not use — an
// engine disagreement about *correctness*. This is the check that would catch
// the next one.
//
// # The two shapes of a case (M7 P06)
//
// The RV32 shape is one entry: `entry`, `cycle`, `instret`, `end`, the watch
// pair, `initial` (the whole memory from offset zero, base64), `calls` and
// `expect`. The exit protocol's fields sit at RV32's fixed offsets and the
// recorded register file is 32 words compared from `x1`.
//
// The Xtensa shape is the same file generalised: a `layout` block says how
// many words the architectural head is (64 `AR` plus the eight extra words),
// from which index the words are compared, and where the counters, flags and
// status sit past it; `ranges` replaces `initial` (the arena has a 200 MiB
// gap nothing reads); `module` names a shared module file; and `entries` is
// a list, each with its own `regsIn`, a memory `delta` the interpreter wrote
// between entries, its calls and its `expect`. A `step` call carries the whole
// head as `regs`. Absent fields take the RV32 values, so every existing case
// reads exactly as it did.

import { readFileSync, readdirSync, existsSync } from "node:fs";
import { join } from "node:path";

const dir = process.argv[2];
if (!dir) {
  console.error("usage: jit-engine-check.mjs <case-directory>");
  process.exit(2);
}

/** The 64-bit FNV-1a the Rust side computes, so the two agree by construction. */
function fnv1a(bytes, h = 0xcbf29ce484222325n) {
  const prime = 0x100000001b3n;
  const mask = 0xffffffffffffffffn;
  for (let i = 0; i < bytes.length; i++) {
    h = (h ^ BigInt(bytes[i])) & mask;
    h = (h * prime) & mask;
  }
  return h;
}

/** Where the exit protocol's fields sit — see `lp-emu-jit`'s `host` module. */
const RV32_LAYOUT = { words: 32, compareFrom: 1, cycle: 128, instret: 136, flags: 144, status: 148 };

function runCase(name) {
  const spec = JSON.parse(readFileSync(join(dir, `${name}.json`), "utf8"));
  const modulePath = join(dir, spec.module ?? `${name}.wasm`);
  const wasm = readFileSync(modulePath);
  const L = { ...RV32_LAYOUT, ...(spec.layout ?? {}) };

  const memory = new WebAssembly.Memory({ initial: spec.pages });
  const bytes = new Uint8Array(memory.buffer);
  // The initial memory: one blob from zero, or ranges of the imported memory.
  const ranges = spec.ranges
    ? spec.ranges.map((r) => ({ at: r.at, b: Buffer.from(r.b, "base64") }))
    : [{ at: 0, b: Buffer.from(spec.initial, "base64") }];
  let h = 0xcbf29ce484222325n;
  for (const r of ranges) {
    bytes.set(r.b, r.at);
    h = fnv1a(r.b, h);
  }
  if (h !== BigInt(spec.memoryFnv)) {
    throw new Error(`${name}: the recorded initial memory does not hash to what it says`);
  }
  const hashRanges = () => {
    let h = 0xcbf29ce484222325n;
    for (const r of ranges) h = fnv1a(bytes.subarray(r.at, r.at + r.b.length), h);
    return h;
  };

  const view = new DataView(memory.buffer);
  const x = spec.exchange;
  // One entry (the RV32 shape) or many.
  const entries = spec.entries ?? [
    {
      entry: spec.entry,
      cycle: spec.cycle,
      instret: spec.instret,
      end: spec.end,
      watchLo: spec.watchLo,
      watchHi: spec.watchHi,
      regsIn: null,
      delta: [],
      calls: spec.calls,
      expect: spec.expect,
    },
  ];

  let calls = [];
  let next = 0;
  let entryIndex = 0;
  const take = (kind) => {
    const call = calls[next++];
    if (!call || call.kind !== kind) {
      throw new Error(
        `${name}: entry ${entryIndex}: the module asked for a ${kind} where the recording has ` +
          `${call ? call.kind : "nothing"} (call ${next - 1})`,
      );
    }
    return call;
  };

  // The four imports, replayed. `pc`, `cycle`, `address`, `kind` and `value`
  // are ignored on purpose: the recording is in call order, and checking the
  // arguments here would be checking the recorder against itself. What is
  // being checked is what the module *does* with the answers.
  const imports = {
    emu: {
      memory,
      mmio_load: () => BigInt(take("load").ret),
      // `(status << 32) | pc` since M7b P2: a store's answer is a polling
      // point's answer, so it is an i64 like a load's.
      mmio_store: () => BigInt(take("store").ret),
      poll: () => BigInt(take("poll").ret),
      step_one: () => {
        const call = take("step");
        for (let i = 0; i < L.words; i++) {
          view.setInt32(x + 4 * i, call.regs[i], true);
        }
        view.setBigInt64(x + L.cycle, BigInt(call.cycle), true);
        view.setBigInt64(x + L.instret, BigInt(call.instret), true);
        view.setInt32(x + L.status, call.status, true);
        // The interpreter's own memory writes, at the granularity
        // `lp-emu-jit`'s `replay` module defines. Without these the replay
        // runs the module against memory the real run never had — the spike's
        // entry-21 divergence, and the difference between a replay that proves
        // something and one that lies.
        for (const g of call.mem ?? []) {
          bytes.set(Buffer.from(g.b, "base64"), g.at);
        }
        return call.pc;
      },
    },
  };

  const module = new WebAssembly.Module(wasm);
  const instance = new WebAssembly.Instance(module, imports);

  const bad = [];
  const eq = (what, got, expected) => {
    if (String(got) !== String(expected)) bad.push(`${what}: got ${got}, recorded ${expected}`);
  };
  for (entryIndex = 0; entryIndex < entries.length; entryIndex++) {
    const e = entries[entryIndex];
    const tag = entries.length > 1 ? `entry ${entryIndex} ` : "";
    for (const g of e.delta ?? []) bytes.set(Buffer.from(g.b, "base64"), g.at);
    if (e.regsIn) {
      for (let i = 0; i < L.words; i++) view.setInt32(x + 4 * i, e.regsIn[i], true);
    }
    view.setBigInt64(x + L.cycle, BigInt(e.cycle), true);
    view.setBigInt64(x + L.instret, BigInt(e.instret), true);
    calls = e.calls;
    next = 0;
    const pc = instance.exports.run(
      e.entry,
      BigInt(e.cycle),
      BigInt(e.instret),
      BigInt(e.end),
      BigInt(e.watchLo),
      BigInt(e.watchHi),
    );
    const want = e.expect;
    eq(`${tag}exit pc`, pc >>> 0, want.pc >>> 0);
    eq(`${tag}flags`, view.getInt32(x + L.flags, true), want.flags);
    eq(`${tag}cycle`, view.getBigInt64(x + L.cycle, true), BigInt(want.cycle));
    eq(`${tag}instret`, view.getBigInt64(x + L.instret, true), BigInt(want.instret));
    for (let i = L.compareFrom; i < L.words; i++) {
      eq(`${tag}word ${i}`, view.getInt32(x + 4 * i, true), want.regs[i]);
    }
    if (next !== calls.length) {
      bad.push(`${tag}imports: the module made ${next} calls, the recording has ${calls.length}`);
    }
    if (bad.length > 8) break;
  }
  const finalFnv = spec.entries ? spec.expect.memoryFnv : entries[0].expect.memoryFnv;
  eq("memory", hashRanges(), BigInt(finalFnv));
  return bad;
}

const names = [
  ...new Set(
    readdirSync(dir)
      .filter((f) => f.endsWith(".json"))
      .map((f) => f.slice(0, -5))
      .filter((n) => existsSync(join(dir, `${n}.json`))),
  ),
].sort();
if (names.length === 0) {
  console.error(`jit-engine-check: no cases in ${dir}`);
  process.exit(1);
}

const engine = typeof Bun !== "undefined" ? `bun ${Bun.version} / JavaScriptCore` : `node ${process.version} / V8`;
let failed = 0;
for (const name of names) {
  let bad;
  try {
    bad = runCase(name);
  } catch (e) {
    bad = [String(e && e.message ? e.message : e)];
  }
  if (bad.length === 0) {
    console.log(`  ok    ${name}`);
  } else {
    failed++;
    console.log(`  FAIL  ${name}`);
    for (const line of bad) console.log(`          ${line}`);
  }
}
console.log(`${engine}: ${names.length - failed}/${names.length} cases match the wasmtime run`);
process.exit(failed === 0 ? 0 : 1);
