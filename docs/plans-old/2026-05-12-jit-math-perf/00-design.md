# JIT Math Performance Design

## Scope Of Work

Build a data-driven math performance spike for the ESP32-C6 Q32 JIT hot path. The work starts with an on-device measurement harness, then implements only the fast math changes justified by those numbers:

- Firmware-side math microbenchmarks for primitive and table-access costs.
- Candidate fast kernels for division and trig, with accuracy summaries.
- A normal-rendering compiler path that defaults to fast math.
- Steady-render profiles before and after the selected changes.

The design does not remove the on-device GLSL compiler, does not make shader compilation optional, and does not gate compiler code behind `std`.

## File Structure

```text
docs/plans/2026-05-12-jit-math-perf/
  00-notes.md
  00-design.md
  01-firmware-math-harness.md
  02-candidate-math-kernels.md
  03-compiler-fast-math-path.md
  04-steady-render-validation.md
  05-cleanup-and-decision.md
  future.md

lp-fw/fw-esp32/
  Cargo.toml
  src/main.rs
  src/tests/jit_math_perf/
    mod.rs
    cycle_counter.rs
    corpus.rs
    runner.rs
    div_kernels.rs
    mul_kernels.rs
    trig_kernels.rs
    lut_cost.rs

lp-shader/lps-q32/src/
  q32_options.rs
  fast_math.rs                  # only if selected kernels need shared policy helpers

lp-shader/lps-builtins/src/builtins/glsl/
  sin_q32.rs
  cos_q32.rs
  sincos_q32.rs
  fast_trig_lut_q32.rs          # only if the LUT candidate wins
  fast_trig_lut_q32_data.rs     # generated or const-generated table data

lp-shader/lpvm-native/src/
  vinst.rs
  isa/rv32/encode.rs
  isa/rv32/emit.rs
  lower.rs

lp-shader/lpir/src/
  const_fold.rs                 # or new granular pass file if const-divisor rewrite grows
  compiler_config.rs

lp-core/lpc-model/src/nodes/shader/
  glsl_opts.rs

lp-core/lpc-engine/src/nodes/shader/
  shader_node.rs

examples/basic/shader.toml
examples/rocaille/shader.toml
examples/perf/fastmath/shader.toml
justfile
```

## Architecture Summary

The spike has three layers:

1. **Hardware math lab.** A new `fw-esp32` feature, `test_jit_math_perf`, runs directly on ESP32-C6. It configures the PMU cycle counter, executes warmed benchmark loops, subtracts loop overhead, and prints median/min/max cycle summaries for:
   - existing builtin helpers,
   - inline-style candidate kernels,
   - constant-divisor reciprocal kernels,
   - trig approximation candidates,
   - flash-vs-RAM LUT access patterns.
   Hardware flash/run commands must target `/dev/cu.usbmodem1101` explicitly.
   The current RV32 cargo runner in `lp-fw/fw-esp32/.cargo/config.toml` invokes
   `espflash flash --chip esp32c6 --monitor --after hard-reset`, so the plan's
   hardware recipe should set `ESPFLASH_PORT=/dev/cu.usbmodem1101` or pass the
   equivalent espflash port flag.

2. **Candidate library shape.** Experimental kernels start inside the firmware test module so they do not become product API prematurely. Any candidate selected for shipping is moved into the appropriate no-std crate:
   - pure Q32 policy/helpers in `lps-q32`,
   - builtin C ABI functions in `lps-builtins`,
   - backend instruction expansion in `lpvm-native`.
   LUT data should be generated reproducibly using an ignored host test or build-time generator, following existing psrdnoise/gnoise precedent.

3. **Compiler/runtime adoption.** Normal shader compilation defaults to the fastest supported math path:
   - wrapping add/sub/mul,
   - reciprocal divide,
   - selected fast trig builtin(s),
   - selected inline reciprocal/const-divisor lowering if data supports it.
   Reference/saturating helpers remain as test or future debug-probe references, not as normal rendering mode.

## Main Components And Interactions

- `jit_math_perf::cycle_counter`
  - Owns ESP32-C6 PMU setup and `read_cycle()`.
  - Mirrors the `test_msafluid` PMU comments so future readers know why `mcycle` is not used.

- `jit_math_perf::runner`
  - Runs fixed warmup and measured iterations.
  - Uses `core::ptr::read_volatile` / `write_volatile` or `core::hint::black_box` where available to prevent dead-code elimination.
  - Prints compact tables with enough labels to paste into `docs/reports`.

- `jit_math_perf::corpus`
  - Provides deterministic Q32 input sets:
    - normal shader values around `[-4, 4]`,
    - angles across several turns,
    - near-zero divisors,
    - divisors used by real shaders (`2`, `3`, `6`, palette/ramp constants),
    - edge values for overflow behavior.

- `jit_math_perf::div_kernels`
  - Baselines `__lp_lpir_fdiv_recip_q32`.
  - Measures inline-style reciprocal divide, constant-divisor reciprocal multiply, and power-of-two shifts.
  - Separates arbitrary runtime divisors from constants, because the known highest-leverage win is avoiding the runtime `divu`.

- `jit_math_perf::mul_kernels`
  - Verifies helper multiply versus wrapping inline multiply.
  - Keeps this section short unless the numbers show an unexpected cost.

- `jit_math_perf::trig_kernels`
  - Compares current Taylor sine/cosine to cheaper candidate families:
    - smaller polynomial,
    - parabolic/cubic approximation,
    - LUT nearest,
    - LUT linear interpolation,
    - paired `sincos` where useful.
  - Captures quality summary on the same corpus.

- `jit_math_perf::lut_cost`
  - Measures read-only table access from rodata and copied RAM tables.
  - Sweeps candidate sizes, at minimum 256, 512, 1024, and 2048 entries for Q32 or i16 table encodings.
  - Includes sequential, strided, and pseudo-random access.

- `lpvm-native` fast lowering
  - Keep lowering local and explicit.
  - Add `AluOp::MulHu` if reciprocal divide is inlined on native.
  - Keep helper functions as reference implementations and tests.

- `lpir` fast math / const rewrite
  - If constant divisors are common in generated LPIR, add a narrow pass that rewrites `Fdiv(x, const)` in reciprocal mode to a precomputed reciprocal multiply or shift.
  - Keep the pass no-std and independent of host-only tooling.

## Validation Philosophy

- Hardware PMU data decides primitive math choices.
- Host tests decide accuracy and bit-equivalence where exactness is promised.
- Existing filetests protect shader semantics.
- Steady-render profiles decide whether the selected primitive wins matter in the actual product workload.
