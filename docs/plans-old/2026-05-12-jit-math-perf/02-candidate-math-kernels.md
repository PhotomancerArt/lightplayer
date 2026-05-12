# Phase 2: Candidate Math Kernels

## Scope Of Phase

Add and measure candidate fast kernels inside the firmware math harness, plus host-side accuracy checks where practical. This phase answers which candidates deserve product/compiler integration.

Out of scope:

- Changing normal shader compilation defaults.
- Moving experimental kernels into product modules before a winner is chosen.
- Building the later debug math probe.

## Code Organization Reminders

- Keep experimental ESP32-only kernels under `lp-fw/fw-esp32/src/tests/jit_math_perf/`.
- Use search-friendly names like `div_kernels.rs`, `trig_kernels.rs`, and `lut_cost.rs`.
- Put the benchmark entry points near the top of each file; helper math lower in the file.
- Mark temporary code with `TODO(math-perf)` only when it truly must be removed later.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

### Division candidates

In `lp-fw/fw-esp32/src/tests/jit_math_perf/div_kernels.rs`, measure:

- Baseline `__lp_lpir_fdiv_recip_q32`.
- Inline-style arbitrary reciprocal divide mirroring `fdiv_recip_q32`:
  - divisor zero guard,
  - unsigned absolute values,
  - `0x8000_0000u32 / abs_divisor`,
  - unsigned wide multiply and shift,
  - sign application.
- Constant-divisor reciprocal multiply for representative constants:
  - precompute `recip2 = (0x8000_0000u32 / abs_divisor) * 2`.
  - runtime uses wide multiply and shift only.
- Power-of-two divisors as shifts, including sign behavior checks.

Do not spend time on Newton-Raphson for arbitrary divisors unless the baseline numbers are surprising. Prior notes already suggest hardware `divu` plus wide multiply is the obvious arbitrary-divisor path on ESP32-C6.

### Multiplication candidates

In `mul_kernels.rs`, measure:

- Baseline `__lp_lpir_fmul_q32`.
- Wrapping multiply equivalent to the current native lowering.
- A direct Rust `((a as i64 * b as i64) >> 16) as i32` version as a compiler-codegen comparison point.

The expected result is confirmation that the existing native inline path is already right. If confirmed, document that and move on.

### Trig candidates

In `trig_kernels.rs`, measure and quality-check:

- Current `__lps_sin_q32`.
- Current `lps_sincos_q32_pair` when both sine and cosine are needed.
- Reduced polynomial candidates.
- Parabolic/cubic shader-quality candidates.
- LUT nearest candidates:
  - 256, 512, 1024, and 2048 samples.
- LUT linear interpolation candidates:
  - same sizes, with table entries stored as either `i16` or `i32` if useful.
- Optional paired `sincos` LUT lookup, returning both values from one range reduction.

For each candidate, print:

- median cycles per call,
- cycles per corpus sweep,
- max absolute error,
- RMS error,
- worst input value,
- table size and placement.

Keep the first acceptance budget pragmatic:

- `sin(0)`, `sin(π/2)`, `sin(π)`, `sin(-π/2)` must remain visually sane.
- Max absolute error should be reported against current builtin and against host `libm` reference.
- Candidates around or below the current 3% trig tolerance are preferred, but a visibly harmless higher error can be considered if the cycle win is large.

### LUT cache / memory candidates

In `lut_cost.rs`, measure:

- rodata table reads,
- RAM-copied table reads,
- sequential, strided, and pseudo-random patterns,
- table sizes that correspond to trig candidates.

The goal is to answer whether LUT trig is actually faster on ESP32-C6 after instruction cache / data cache / flash access behavior is included.

### Host accuracy support

If useful, add a narrow host test module in `lps-builtins` or `lps-q32` for the candidate chosen by hardware data. Do not move every experimental candidate into shared crates just to test it.

If LUT generation is needed, follow existing patterns:

- `lp-shader/lps-builtins/src/builtins/lpfn/generative/psrdnoise/grad_lut_q32.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/psrdnoise/fibonacci_lut_q32.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/gnoise/smooth_lut_q32.rs`

Prefer a reproducible ignored test or const generator. Avoid hand-writing large tables without a regeneration path.

## Validate

Run:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features test_jit_math_perf,esp32c6
cargo test -p lps-builtins
cargo test -p lps-q32
```

On hardware, run:

```bash
ESPFLASH_PORT=/dev/cu.usbmodem1101 just fwtest-jit-math-perf-esp32c6
```

Record the resulting numbers in either:

```text
docs/reports/2026-05-12-jit-math-perf.md
```

or a later dated report if the measurements happen on a different day.
