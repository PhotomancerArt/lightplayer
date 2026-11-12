# Phase 3: Output And Node Migration

## Scope Of Phase

Move output onto the shared demand-root `consume()` path and migrate remaining runtime nodes onto the new contract.

Out of scope:

- Behavior changes unrelated to the runtime contract.
- Large internal rewrites of shader, fixture, playlist, texture, or fluid logic.

## Code Organization Reminders

- Keep output sink flushing in `EngineServices`.
- Keep each node's production helpers in that node's existing file.
- Prefer small helpers like `evaluate_full_node` when using compatibility-style production.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/nodes/output/output_node.rs`
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

Expected changes:

- Move `OutputNode::tick()` body to `consume()`.
- Remove output's dummy `in` demand-root binding once the engine calls demand-root `consume()` directly.
- Migrate simple nodes by using explicit `produce(slot)` or the once-per-frame full-evaluation helper.
- Keep node behavior unchanged except for the new dispatch shape.

Suggested node guidance:

- `ButtonNode`: `produce(down/held/up)` samples/publishes button event maps.
- `ClockNode`: `produce(...)` publishes time state.
- `PlaylistNode`: `produce(output)` resolves trigger/time and selected child output.
- `FixtureNode`: `produce(output)` resolves its visual/control input and publishes fixture output.
- Shader/compute/texture/fluid nodes: use full-evaluation helper unless slot-specific production is obvious and low risk.
- Placeholder nodes: return unsupported/no-op as appropriate.

## Validate

```bash
cargo fmt --package lpc-engine
cargo test -p lpc-engine output_demand_marks_output_buffer_dirty_same_frame_before_flush
cargo test -p lpc-engine engine_output_sink_flush_writes_expected_rgb_via_memory_provider
cargo test -p lpc-engine
cargo test -p lpc-engine --test runtime_spine
```
