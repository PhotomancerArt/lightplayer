// The JS half of the browser seam: what an emulator instance needs so the
// translator inside it can run its own output in the engine hosting it.
//
// **This module is the product, not the rig.** JD25 makes Studio's `?on=emu`
// worker import exactly this file, so it knows nothing about the bench page,
// takes no options it does not need, and does no I/O. Using it is three steps:
//
//     import { makeJitHost } from './jit-host.js';
//
//     const host = makeJitHost();
//     const inst = await WebAssembly.instantiate(module, {
//       ...wasi.imports,          // your own WASI preview1 shim
//       ...host.imports,          // the `emu_host` namespace, below
//     });
//     host.attach(inst);          // BEFORE _start(); throws if the seam is wrong
//     inst.exports._start();
//
// and `host.events` afterwards is one entry per translation event, which is
// what a boot-cost line (JD20) is read off.
//
// ## What it provides, and what it must never do
//
// Two imports, in the namespace `emu_host`, called **once per translation
// event** (JD5: boot, then each `fence.i`) and never on a hot path:
//
// | import | shape | does |
// |---|---|---|
// | `jit_compile` | `(ptr, len, base, timings) -> i32` | compile, instantiate, install; returns a table slot or a negative error |
// | `jit_release` | `(idx)` | null the slot so the next event can replace the module |
//
// Everything else is wasm→wasm. The translated module's `mmio_load`,
// `mmio_store` and `step_one` imports are bound directly to the emulator
// instance's own exports and its `memory` to the emulator's own linear memory,
// so no JS frame sits on any path the guest runs through. P2 measured the
// difference: 1.60 ns for a wasm→wasm MMIO call against 6.05 ns through a JS
// shim, and 5.40 ns for a table entry against 9.00 ns for a host import.
//
// ## The synchronous compile is deliberate
//
// `new WebAssembly.Module` here is synchronous, at 64 MB, and that is legal
// rather than merely tolerated: guest time is the scheduler's and the wall
// clock never enters the machine (PD5 / ADR 2026-09-06), so however long a
// compile takes it cannot reach a transcript. What it *can* do is freeze a
// thread — which is why **the emulator must run in a dedicated Worker**. JD13
// makes that a correctness requirement, not a convenience, and this module
// will happily do the wrong thing if it is imported on the main thread.
//
// ## `table.grow(1)` then `table.set(idx, ref)`, never the fused form
//
// JavaScriptCore accepts `table.grow(delta, initValue)` for a funcref sourced
// from another instance, reports the funcref back from `table.get(idx)` — and
// `call_indirect` on that same slot traps as a null entry. No exception, no
// warning, and the JS-visible reflection disagreeing with the bytecode-visible
// table. P2 reproduced it in three lines, independently of the rest of its
// probe. Two calls, always.
'use strict';

/// The negative returns of `jit_compile`. Mirrors
/// `lp_emu_jit::host_browser::compile_error`; those two lists are the only
/// places these numbers appear.
export const COMPILE_ERROR = {
  COMPILE: -1,
  INSTANTIATE: -2,
  NO_ENTRY: -3,
  TABLE: -4,
  NO_HOST: -5,
};

/// The constant `jit_table_probe` folds in, so a wrong slot cannot
/// accidentally produce the right answer.
const PROBE_XOR = 0x5a5a5a5a | 0;

export function makeJitHost() {
  let inst = null;
  let table = null;
  let selftest = null;             // what `attach` proved, for the report
  const live = new Map();          // slot -> { module, instance, bytes }
  const events = [];               // one per translation event

  const memory = () => inst.exports.memory;

  function jit_compile(ptr, len, base, timings) {
    if (!inst) return COMPILE_ERROR.NO_HOST;
    const event = { bytes: len, base, compileMs: 0, instantiateMs: 0, slot: -1, error: null };
    events.push(event);

    // A view, not a copy: `new WebAssembly.Module` reads it synchronously and
    // copies what it keeps, and copying 64 MB first would double the peak.
    const wasm = new Uint8Array(memory().buffer, ptr, len);

    let t0 = performance.now(), module;
    try {
      module = new WebAssembly.Module(wasm);
    } catch (e) {
      event.error = String((e && e.message) || e);
      return COMPILE_ERROR.COMPILE;
    }
    event.compileMs = performance.now() - t0;

    t0 = performance.now();
    let instance;
    try {
      instance = new WebAssembly.Instance(module, {
        emu: {
          memory: memory(),
          mmio_load: inst.exports.jit_mmio_load,
          mmio_store: inst.exports.jit_mmio_store,
          step_one: inst.exports.jit_step_one,
        },
      });
    } catch (e) {
      event.error = String((e && e.message) || e);
      return COMPILE_ERROR.INSTANTIATE;
    }
    event.instantiateMs = performance.now() - t0;

    const run = instance.exports.run;
    if (typeof run !== 'function') {
      event.error = 'the module has no `run` export';
      return COMPILE_ERROR.NO_ENTRY;
    }

    let idx;
    try {
      idx = table.length;
      table.grow(1);                 // NEVER table.grow(1, run) — see the header
      table.set(idx, run);
    } catch (e) {
      event.error = String((e && e.message) || e);
      return COMPILE_ERROR.TABLE;
    }
    live.set(idx, { module, instance, bytes: len });
    event.slot = idx;

    // The memory can have grown while the engine compiled, which detaches
    // every view of it, so the DataView is taken now rather than reused.
    const dv = new DataView(memory().buffer);
    dv.setFloat64(timings, event.compileMs, true);
    dv.setFloat64(timings + 8, event.instantiateMs, true);
    return idx;
  }

  function jit_release(idx) {
    if (!live.has(idx)) return;
    live.delete(idx);
    // The slot is nulled, not reclaimed: a `WebAssembly.Table` does not
    // shrink. Three translation events cost three slots for the life of the
    // run, which is the whole cost of not reusing one.
    try { table.set(idx, null); } catch { /* a detached table is the engine going away */ }
  }

  return {
    imports: { emu_host: { jit_compile, jit_release } },

    /// Wire the host to a freshly instantiated emulator, and **prove the entry
    /// mechanism in this engine before anything depends on it**.
    ///
    /// The round trip is the one the brief asks for: grow the emulator's own
    /// `__indirect_function_table`, write `jit_table_probe` — an export of the
    /// emulator module, reached here as a JS funcref, exactly as a translated
    /// module's `run` will be — into the new slot, and ask the emulator to
    /// `call_indirect` it by index. If "a function pointer is a table index"
    /// does not hold, or if the engine's table write does not reach its
    /// bytecode-visible table (P2's fused-`grow` bug), this throws here rather
    /// than trapping half a minute later inside a 64 MB module.
    ///
    /// Call it **before `_start()`**: the probe is a leaf function that
    /// allocates nothing, and the stack pointer a wasip1 module needs is a
    /// global initialised at instantiation.
    attach(instance) {
      inst = instance;
      table = instance.exports.__indirect_function_table;
      if (!table) {
        throw new Error(
          'the emulator module does not export __indirect_function_table — ' +
          'build it with -C link-arg=--export-table');
      }
      try {
        table.grow(0);
        const probe = table.length;
        table.grow(1);
        table.set(probe, null);
      } catch (e) {
        throw new Error(
          'the emulator module\'s __indirect_function_table cannot grow (' + ((e && e.message) || e) +
          ') — build it with -C link-arg=--growable-table, which is what removes the maximum ' +
          'wasm-ld otherwise pins to the table\'s initial size');
      }
      for (const name of ['jit_mmio_load', 'jit_mmio_store', 'jit_step_one', 'jit_table_probe', 'jit_table_selftest']) {
        if (typeof instance.exports[name] !== 'function') {
          throw new Error('the emulator module does not export ' + name + ' — build it with --features jit');
        }
      }
      const idx = table.length;
      table.grow(1);
      table.set(idx, instance.exports.jit_table_probe);
      const x = 0x1234;
      const got = instance.exports.jit_table_selftest(idx, x);
      const want = (x ^ PROBE_XOR) | 0;
      table.set(idx, null);
      if (got !== want) {
        throw new Error(
          'the function-table entry mechanism does not hold in this engine: slot ' + idx +
          ' answered ' + got + ', expected ' + want);
      }
      selftest = { slot: idx, got, want, ok: true };
      return selftest;
    },

    /// One entry per translation event: module bytes, the arena base they were
    /// emitted against, the engine's own compile and instantiate milliseconds,
    /// the table slot, and the error if there was one. JD20's boot-cost line,
    /// from the JS side.
    events,
    get selftest() { return selftest; },
    get slots() { return live.size; },
  };
}
