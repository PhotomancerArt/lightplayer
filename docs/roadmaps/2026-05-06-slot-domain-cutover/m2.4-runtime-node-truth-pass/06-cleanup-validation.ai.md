# Phase 6: Cleanup And Validation

## Scope Of Phase

Finish the milestone by removing stale texture-flow assumptions, tightening names/docs, and running focused validation.

In scope:

- Remove unused helpers such as texture dimension query helpers if no longer used.
- Remove stale comments saying shader output is `texture`.
- Rename test helpers from texture/shader coupling to render-flow terms.
- Remove dead imports and warnings.
- Keep `TextureNode` only if it compiles cleanly and has a clear isolated test.
- Update M2.4 summary if the repo convention expects one.

Out of scope:

- Broad formatting churn across unrelated files.
- M3 canonical sync rebuild.
- UI work.

## Code Organization Reminders

- Preserve concept-per-file organization.
- Keep docs attached to the types they explain.
- Tests stay at file bottoms.
- Avoid commented-out experiments.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Search targets:

```bash
rg "texture_node_id|shader_node_id|shader_texture_output_path|texture_dimension_query_targets|fixture_projection_info|shader_projection_wire|FixtureProjectionInfo|ShaderProjectionWire" lp-core/lpc-engine/src lp-core/lpc-engine/tests
```

Expected cleanup:

- No projection hook symbols remain.
- No fixture runtime dependence on a texture node remains.
- No shader runtime dependence on texture dimensions remains.
- Canonical tests and examples use shader `output` and fixture `input`.
- Any retained texture node is clearly isolated as non-canonical/future-ish support.

## Validate

```bash
cargo check -p lpc-model
cargo check -p lpc-engine
cargo test -p lpc-engine node::
cargo test -p lpc-engine engine::
cargo test -p lpc-engine project_runtime::
cargo test -p lpc-engine --test runtime_spine
cargo test -p lpc-source --test basic_example_parse
```
