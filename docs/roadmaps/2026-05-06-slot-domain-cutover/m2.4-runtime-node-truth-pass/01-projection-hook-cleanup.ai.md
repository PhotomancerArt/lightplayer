# Phase 1: Projection Hook Cleanup

## Scope Of Phase

Remove dead legacy projection hooks from the runtime node surface before changing render flow.

In scope:

- Remove `NodeRuntime::fixture_projection_info`.
- Remove `NodeRuntime::shader_projection_wire`.
- Remove `FixtureProjectionInfo`.
- Remove `ShaderProjectionWire`.
- Remove concrete impls from `FixtureNode` and `ShaderNode`.
- Remove imports that only existed for those projection hooks.

Out of scope:

- Changing render-product behavior.
- Changing authored TOML.
- Changing loader graph resolution.
- Removing `runtime_output_sink_buffer_id`.
- Removing `primary_render_product_id` unless it is proven unused by this phase.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep helpers lower in the file when that improves readability.
- Put tests at the bottom of files.
- Do not preserve dead projection structs as renamed compatibility types.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/node/node_runtime.rs`
- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`
- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-core/lpc-engine/src/project_runtime/*`

Expected changes:

- Delete projection hook methods and related structs from `NodeRuntime`.
- Delete `FixtureNode::fixture_projection_info`.
- Delete `ShaderNode::shader_projection_wire`.
- Search for `fixture_projection_info`, `shader_projection_wire`,
  `FixtureProjectionInfo`, and `ShaderProjectionWire`; no references should
  remain.
- Keep `runtime_output_sink_buffer_id` because output flushing still depends on it.
- Keep `primary_render_product_id` for now if shader tests/API helpers still use it.

## Validate

```bash
cargo check -p lpc-engine
cargo test -p lpc-engine node::
cargo test -p lpc-engine engine::
```
