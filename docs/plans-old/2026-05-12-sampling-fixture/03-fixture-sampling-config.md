# Phase 3: Fixture Sampling Config

## Scope Of Phase

Make fixture visual evaluation an authored strategy.

In scope:
- Add `FixtureSamplingConfig`.
- Move texture-specific `render_size` and `sample_diameter` under `texture_area`.
- Add direct sampling variant.
- Keep `kind = "fixture"`.
- Update TOML parsing tests.

Out of scope:
- Runtime strategy implementation beyond compiling.
- Converting `examples/basic`.

## Code Organization Reminders

- Put `FixtureSamplingConfig` in `lpc-model/src/nodes/fixture/sampling.rs`.
- Keep `mapping.rs` focused on fixture geometry/path data.
- Keep rsdocs precise about strategy semantics.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and deviations.

## Implementation Details

Relevant files:
- `lp-core/lpc-model/src/nodes/fixture/fixture_def.rs`
- `lp-core/lpc-model/src/nodes/fixture/mapping.rs`
- `lp-core/lpc-model/src/nodes/fixture/mod.rs`
- generated slot view code if needed
- `examples/basic/fixture.toml` later, not in this phase unless needed for tests

Expected TOML:

```toml
[sampling]
kind = "direct"
```

or:

```toml
[sampling]
kind = "texture_area"
render_size = { width = 16, height = 16 }
sample_diameter = 2.0
```

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model fixture -- --nocapture
cargo check -p lpc-model --features schema-gen
```

