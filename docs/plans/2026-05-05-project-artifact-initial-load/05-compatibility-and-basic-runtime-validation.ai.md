# Phase 5: Compatibility Projection And Basic Runtime Validation

## Scope of phase

Make the new project-artifact-loaded `examples/basic` runtime behave like the old initial scene for current client/detail/resource paths.

In scope:

- Populate `CompatibilityProjection` from artifact-loaded `*Def` values.
- Ensure `NodeDetail`/legacy compatibility projection still returns texture/shader/output/fixture details for watched nodes.
- Ensure resource summaries/payloads still work for shader render products, output buffers, and fixture buffers.
- Update focused `lpc-engine` scene render/resource tests for the new basic layout and loader.

Out of scope:

- Do not redesign the wire model.
- Do not remove legacy wire `NodeState` structs.
- Do not migrate all server/CLI tests yet unless they directly block this validation.

## Code Organization Reminders

- Follow the repo rule: top to bottom is most important to least important, with tests at the bottom of each Rust file.
- Prefer one concept per file and keep related functionality grouped together.
- Keep helper functions below the public/primary API they support.
- Any temporary code must have a searchable TODO comment and should be removed by the cleanup phase.
- Preserve no_std compatibility in `lpc-model`, `lpc-source`, `lpc-engine`, and shader/runtime paths. Do not add std gates to compile/execute paths.

## Codex / Worker Reminders

- Do not commit. The plan commits at the end as a single unit unless the user explicitly says otherwise.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]` to make the build pass. Fix the issue.
- Do not disable, skip, or weaken existing tests.
- If blocked by ambiguity or an unexpected design issue, stop and report back rather than improvising.
- Report back with: what changed, what was validated, and any deviations from this phase.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/project_runtime/compatibility_projection.rs`
- `lp-core/lpc-engine/src/project_runtime/detail_projection.rs`
- `lp-core/lpc-engine/src/project_runtime/resource_projection.rs`
- `lp-core/lpc-engine/src/project_runtime/core_project_runtime.rs`
- `lp-core/lpc-engine/tests/scene_render.rs`
- `lp-core/lpc-engine/tests/partial_state_updates.rs`
- `lp-core/lpc-engine/tests/get_changes_resource_projection.rs`

Compatibility projection should be considered temporary. It should adapt to the new source of truth without reintroducing directory discovery or `/src/*.kind` semantics.

Tests should prove:

- `examples/basic` loads through `/project.toml`.
- A render tick produces a shader render product.
- Watched node details include semantic refs/resource fields where they did before.
- Partial/detail state updates still work for the basic scene.

## Validate

```bash
cargo test -p lpc-engine --test scene_render --test partial_state_updates --test get_changes_resource_projection
```
