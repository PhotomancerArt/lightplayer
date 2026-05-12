# Phase 6: Example And Perf Validation

## Scope Of Phase

Switch the canonical example to direct sampling and add evidence that the new path is active.

In scope:
- Update `examples/basic/fixture.toml` to use `[sampling] kind = "direct"`.
- Add or update tests proving the example loads.
- Add lightweight perf/evidence logging or tests that distinguish direct sampling from texture-area rendering.

Out of scope:
- Final UI.
- Removing old example/test coverage for texture-area fixtures.

## Code Organization Reminders

- Keep example TOML readable.
- Avoid permanent noisy logs unless they are already part of useful dev diagnostics.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and deviations.

## Implementation Details

Relevant files:
- `examples/basic/fixture.toml`
- project loader tests
- fixture/output engine tests

Expected changes:
- `examples/basic` uses direct sampling.
- Existing texture-area behavior remains covered by focused tests.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine project_toml -- --nocapture
cargo test -p lpc-engine output_ -- --nocapture
cargo check -p lp-cli
```

