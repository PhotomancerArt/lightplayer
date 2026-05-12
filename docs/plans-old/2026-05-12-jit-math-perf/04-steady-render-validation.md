# Phase 4: Steady Render Validation

## Scope Of Phase

Measure whether the selected math changes improve the actual steady-render target. This phase compares before/after profiles and checks that shader output remains acceptable.

Out of scope:

- New optimization work beyond small fixes needed to make the selected changes work.
- Broad runtime graph or allocation optimization.
- Reworking `psrdnoise`.

## Code Organization Reminders

- Keep reports under `docs/reports/`.
- Keep profile directories under `profiles/`.
- Do not edit generated profile output by hand.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Run and save steady-render profiles for at least:

- `examples/basic`
- `examples/rocaille`
- `examples/perf/fastmath` if it still exists as a useful stress case.

Use the existing profile command shape:

```bash
cargo run -p lp-cli -- profile examples/basic --mode steady-render --note jit-math-perf
cargo run -p lp-cli -- profile examples/rocaille --mode steady-render --note jit-math-perf
cargo run -p lp-cli -- profile examples/perf/fastmath --mode steady-render --note jit-math-perf
```

If the exact CLI args differ, inspect `lp-cli/src/commands/profile/args.rs` and use the current accepted form. Do not run `cargo build --workspace` or `cargo test --workspace`.

Compare:

- total attributed cycles,
- frame-event cycles if present,
- self cycles for:
  - `__lp_lpir_fdiv_recip_q32`
  - `__lps_sin_q32`
  - `__lps_atan2_q32`
  - `[jit] render`
  - `__lp_lpfn_psrdnoise2_q32`
- code size / firmware build output if reported.

Create a report:

```text
docs/reports/2026-05-12-jit-math-perf.md
```

The report should include:

- baseline profile references,
- new profile references,
- hardware PMU primitive table summary,
- selected candidates and why,
- any candidate rejected because cache/table cost erased the expected win,
- final recommendation.

Shader output validation:

- Use existing filetests for semantic guardrails.
- If visual deltas are expected from faster trig, capture a small numeric/sample comparison rather than pretending the output is bit-identical.
- Make sure examples still compile and run.

## Validate

Run:

```bash
cargo run -p lps-filetests-app -- --target rv32n.q32
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

If any listed test target has been renamed on the current branch, find the replacement in `lp-fw/fw-tests/tests/` and record the deviation in the report.
