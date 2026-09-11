#!/usr/bin/env node
// The translated module's answers, checked in a second and a third engine.
//
//   node scripts/emu/jit-engine-check.mjs target/jit-cases    # V8
//   bun  scripts/emu/jit-engine-check.mjs target/jit-cases    # JavaScriptCore
//
// The cases come from `lp-emu-jit`'s own round-trip test, which has already
// asserted them under wasmtime:
//
//   LP_EMU_JIT_ENGINE_CASE=$PWD/target/jit-cases \
//     cargo test -p lp-emu-jit --features host-wasmtime --test translate_roundtrip
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

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const dir = process.argv[2];
if (!dir) {
  console.error("usage: jit-engine-check.mjs <case-directory>");
  process.exit(2);
}

/** The 64-bit FNV-1a the Rust side computes, so the two agree by construction. */
function fnv1a(bytes) {
  let h = 0xcbf29ce484222325n;
  const prime = 0x100000001b3n;
  const mask = 0xffffffffffffffffn;
  for (let i = 0; i < bytes.length; i++) {
    h = (h ^ BigInt(bytes[i])) & mask;
    h = (h * prime) & mask;
  }
  return h;
}

/** Where the exit protocol's fields sit — see `lp-emu-jit`'s `host` module. */
const EXCHANGE_REGS = 0;
const EXCHANGE_CYCLE = 128;
const EXCHANGE_INSTRET = 136;
const EXCHANGE_FLAGS = 144;
const EXCHANGE_STATUS = 148;

function runCase(name) {
  const spec = JSON.parse(readFileSync(join(dir, `${name}.json`), "utf8"));
  const wasm = readFileSync(join(dir, `${name}.wasm`));

  const memory = new WebAssembly.Memory({ initial: spec.pages });
  const bytes = new Uint8Array(memory.buffer);
  const initial = Buffer.from(spec.initial, "base64");
  bytes.set(initial);
  if (fnv1a(initial) !== BigInt(spec.memoryFnv)) {
    throw new Error(`${name}: the recorded initial memory does not hash to what it says`);
  }

  const view = new DataView(memory.buffer);
  const x = spec.exchange;
  const calls = spec.calls;
  let next = 0;
  const take = (kind) => {
    const call = calls[next++];
    if (!call || call.kind !== kind) {
      throw new Error(
        `${name}: the module asked for a ${kind} where the recording has ` +
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
        for (let i = 0; i < 32; i++) {
          view.setInt32(x + EXCHANGE_REGS + 4 * i, call.regs[i], true);
        }
        view.setBigInt64(x + EXCHANGE_CYCLE, BigInt(call.cycle), true);
        view.setBigInt64(x + EXCHANGE_INSTRET, BigInt(call.instret), true);
        view.setInt32(x + EXCHANGE_STATUS, call.status, true);
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
  const pc = instance.exports.run(
    spec.entry,
    BigInt(spec.cycle),
    BigInt(spec.instret),
    BigInt(spec.end),
    BigInt(spec.watchLo),
    BigInt(spec.watchHi),
  );

  const bad = [];
  const want = spec.expect;
  const eq = (what, got, expected) => {
    if (String(got) !== String(expected)) bad.push(`${what}: got ${got}, recorded ${expected}`);
  };
  eq("exit pc", pc >>> 0, want.pc >>> 0);
  eq("flags", view.getInt32(x + EXCHANGE_FLAGS, true), want.flags);
  eq("cycle", view.getBigInt64(x + EXCHANGE_CYCLE, true), BigInt(want.cycle));
  eq("instret", view.getBigInt64(x + EXCHANGE_INSTRET, true), BigInt(want.instret));
  for (let i = 1; i < 32; i++) {
    eq(`x${i}`, view.getInt32(x + EXCHANGE_REGS + 4 * i, true), want.regs[i]);
  }
  eq("memory", fnv1a(bytes.subarray(0, initial.length)), BigInt(want.memoryFnv));
  if (next !== calls.length) {
    bad.push(`imports: the module made ${next} calls, the recording has ${calls.length}`);
  }
  return bad;
}

const names = [
  ...new Set(
    readdirSync(dir)
      .filter((f) => f.endsWith(".wasm"))
      .map((f) => f.slice(0, -5)),
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
