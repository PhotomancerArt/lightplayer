# Phase 6: Fluid Regression and Op Counts

## Scope of phase

Add durable regression coverage for the fluid compute shader and LPIR size behavior.

In scope:

- Compile `examples/fluid/compute.glsl` with its generated header or equivalent test source.
- Assert semantic output for emitted fluid entries.
- Assert LPIR op count stays below a conservative ceiling.
- Add a focused test that `emitters[0].pos` does not produce whole-aggregate select chains.

Out of scope:

- Hardware flashing.
- Broad benchmark harness work.

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep tests at the bottom.
- Avoid brittle exact op counts; use conservative ceilings and targeted absence/presence checks.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-shader/lp-shader/src/tests.rs`
- `lp-shader/lps-glsl/src/lower/place/*`
- `examples/fluid/compute.glsl`

Expected changes:

- Add test coverage for the real fluid compute shader shape.
- Use helper functions to count `LpirOp::Select`, `LpirOp::Store`, and total ops.
- Keep thresholds loose enough to allow harmless implementation changes but tight enough to catch aggregate rebuilds.

## Validate

```bash
cargo fmt --check
cargo test -p lp-shader fluid -- --nocapture
cargo test -p lps-glsl place -- --nocapture
```
