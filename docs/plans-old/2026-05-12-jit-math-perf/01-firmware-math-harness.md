# Phase 1: Firmware Math Harness

## Scope Of Phase

Add a feature-gated ESP32-C6 firmware performance harness for JIT hot-path math. This phase should produce trustworthy on-device cycle numbers for baseline helpers and LUT access overhead, without changing normal shader behavior.

Out of scope:

- Shipping new compiler lowering.
- Replacing builtin math implementations.
- Removing existing `glsl_opts`.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep the `tests/jit_math_perf` directory as the map of the experiment.
- Put helper functions lower in each file and test-only comments near the code they explain.
- Mark any deliberately temporary experiment code with a clear `TODO(math-perf)` tag.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update these files:

- `lp-fw/fw-esp32/Cargo.toml`
  - Add feature `test_jit_math_perf = []`.
  - If direct builtin helper calls require it, add:
    - `lps-builtins = { path = "../../lp-shader/lps-builtins", default-features = false }`
  - Keep the dependency no-std.

- `lp-fw/fw-esp32/src/main.rs`
  - Add `test_jit_math_perf` to the same feature exclusion lists that currently include `test_msafluid` and `test_fluid_demo`.
  - Add:
    ```rust
    #[cfg(feature = "test_jit_math_perf")]
    mod tests {
        pub mod jit_math_perf;
    }
    ```
  - Dispatch to `tests::jit_math_perf::run_jit_math_perf(spawner).await`.

- `lp-fw/fw-esp32/src/tests/jit_math_perf/mod.rs`
  - Declare:
    - `cycle_counter`
    - `corpus`
    - `runner`
    - `div_kernels`
    - `mul_kernels`
    - `trig_kernels`
    - `lut_cost`
  - Export `run_jit_math_perf`.

- `lp-fw/fw-esp32/src/tests/jit_math_perf/cycle_counter.rs`
  - Copy the PMU setup approach from `test_msafluid.rs`:
    - `mpcer` CSR `0x7E0`
    - `mpcmr` CSR `0x7E1`
    - `mpccr` CSR `0x7E2`
  - Keep the comment that explains why standard RISC-V `mcycle` is not used.

- `lp-fw/fw-esp32/src/tests/jit_math_perf/runner.rs`
  - Implement a small benchmark runner:
    - fixed warmup count,
    - fixed measured count,
    - median/min/max/avg,
    - optional loop-overhead calibration,
    - volatile accumulator to avoid dead-code elimination.
  - Print lines with a stable prefix such as `[jit-math-perf]`.

- `lp-fw/fw-esp32/src/tests/jit_math_perf/corpus.rs`
  - Define deterministic Q32 inputs:
    - `ANGLES`: at least one full sweep over `[-4π, 4π]`, plus key angles.
    - `DIVIDENDS`: normal shader values and edge-ish values.
    - `DIVISORS`: constants from real shaders (`0.5`, `1`, `2`, `3`, `6`, `10`, `255`) plus near-zero values.
  - Avoid heap allocation in hot benchmark loops.

- `lp-fw/fw-esp32/src/tests/jit_math_perf/lut_cost.rs`
  - Add rodata tables for size/access experiments.
  - Include at least:
    - sequential read,
    - fixed stride read,
    - pseudo-random read,
    - copied-to-RAM read if RAM cost is feasible.

- `justfile`
  - Add a recipe following the nearby test recipes:
    ```make
    fwtest-jit-math-perf-esp32c6: install-rv32-target
        cd lp-fw/fw-esp32 && ESPFLASH_PORT=/dev/cu.usbmodem1101 cargo run --features test_jit_math_perf,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}
    ```
  - If this repo's `cargo run` path expects a different espflash variable or
    explicit flag, use the existing local convention, but keep the device path
    `/dev/cu.usbmodem1101` explicit in the recipe.

Expected phase output:

- A buildable `test_jit_math_perf` firmware.
- Serial output showing baseline loop overhead and LUT access costs.
- No changes to normal render output.

## Validate

Run:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features test_jit_math_perf,esp32c6
```

On hardware, run:

```bash
ESPFLASH_PORT=/dev/cu.usbmodem1101 just fwtest-jit-math-perf-esp32c6
```

Also run a host sanity check:

```bash
cargo check -p lpa-server
```
