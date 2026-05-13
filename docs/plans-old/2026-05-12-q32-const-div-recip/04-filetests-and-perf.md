# Phase 04: Filetests And Perf Comparison

## Scope Of Phase

Add focused filetests and use per-test cycle counts to verify the const-div optimization is visible and useful.

In scope:

- Add `q32fast-div-const.glsl`.
- Update existing divide filetests only when expectations intentionally change.
- Run focused filetests with `--detail` and record cycle deltas.
- Optionally run a short profile on `examples/basic` or Rocaille if the filetest delta is meaningful.

Out of scope:

- Full profiler exploration.
- Hardware microbench harness changes unless a surprising result needs confirmation.

## Code Organization Reminders

- Keep filetests small and single-purpose.
- Comments should explain approximation expectations, not apologize for them.
- Avoid broad snapshot churn.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-shader/lps-filetests/filetests/scalar/float/q32fast-div-const.glsl`
- `lp-shader/lps-filetests/filetests/scalar/float/q32fast-div-recip.glsl`
- `lp-shader/lps-filetests/filetests/scalar/float/op-divide.glsl`
- `docs/reports/` if adding a short implementation note

Test cases:

- `x / 2.0`
- `x / 3.0`
- `x / -2.0`
- `x / 0.25`
- `vec3(x) / 2.0`
- `vec3(...) / vec3(2.0, 4.0, 8.0)` if Phase 02 supports vector constants
- `const float K = 3.0; x / K`
- `x / 0.0` to ensure fallback still behaves acceptably
- dynamic divisor case for comparison

Cycle comparison should include:

```sh
scripts/filetests.sh --target rv32n.q32 --detail \
  scalar/float/q32fast-div-const.glsl \
  scalar/float/q32fast-div-recip.glsl \
  scalar/float/op-divide.glsl
```

If useful, compare before/after by checking out the parent commit in a throwaway worktree or using git stash carefully. Do not reset or discard user work.

## Validate

```sh
cargo fmt --all
scripts/filetests.sh --target rv32n.q32 --detail \
  scalar/float/q32fast-div-const.glsl \
  scalar/float/q32fast-div-recip.glsl \
  scalar/float/op-divide.glsl
```

Optional profile:

```sh
cargo run -p lp-cli -- profile examples/basic --mode steady-render --note q32-const-div
cargo run -p lp-cli -- profile examples/rocaille --mode steady-render --note q32-const-div
```
