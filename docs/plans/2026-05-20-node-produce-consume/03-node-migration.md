# Phase 3: Node Migration

## Scope Of Phase

Migrate existing runtime nodes from `tick()` to explicit `produce()` and `consume()` implementations.

Out of scope:

- Behavior changes unrelated to the runtime contract.
- Large rewrites of shader, fixture, playlist, or fluid internals.

## Code Organization Reminders

- Keep node-specific helpers in their existing node files.
- Prefer small private helpers like `evaluate_full_node` when a node uses the simple fallback.
- Put tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/nodes/button/button_node.rs`
- `lp-core/lpc-engine/src/nodes/clock/clock_node.rs`
- `lp-core/lpc-engine/src/nodes/playlist/playlist_node.rs`
- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`
- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-core/lpc-engine/src/nodes/shader/compute_shader_node.rs`
- `lp-core/lpc-engine/src/nodes/texture/texture_node.rs`
- `lp-core/lpc-engine/src/nodes/fluid/fluid_node.rs`
- `lp-core/lpc-engine/src/nodes/placeholder/mod.rs`
- `lp-core/lpc-engine/src/engine/test_support.rs`

Expected migration guidance:

- `ButtonNode`: `produce(down/held/up)` samples or publishes event maps without needing demand-root behavior.
- `ClockNode`: `produce(output/time slots)` publishes time state.
- `PlaylistNode`: `produce(output)` resolves trigger/time and selected child output.
- `FixtureNode`: `produce(output)` resolves visual/control inputs and publishes fixture output.
- Shader/compute/texture/fluid nodes: start with full-evaluation fallback unless a slot-specific split is obvious and low risk.
- Placeholder nodes: implement minimal no-op/unsupported production behavior.

Tests to add or update:

- Existing per-node tests should continue to pass.
- Add one test proving a simple fallback node can satisfy multiple produced slots after one evaluation.

## Validate

```bash
cargo fmt --package lpc-engine
cargo test -p lpc-engine
cargo test -p lpc-engine --test runtime_spine
```

