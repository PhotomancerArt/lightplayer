# Phase 4: Output And Loader Cleanup

## Scope Of Phase

Keep `OutputNode` minimal while making generated `OutputDefView` visible and
cleaning loader fallout from thinner runtime constructors.

In scope:

- Add evidence tests for generated `OutputDefView`.
- Keep output service registration loader-side.
- Update loader and runtime tests after shader/fixture constructor changes.
- Audit direct `*Def` convenience-method use and leave only loader/test usage
  that still has a clear purpose.

Out of scope:

- Moving output flushing into node tick.
- Resolver-backed output service mutation.
- Deleting all def convenience methods.

## Code Organization Reminders

- Keep output-node runtime code small.
- Prefer deleting stale helpers over carrying compatibility wrappers.
- Put any future-work notes in `future.md`, not in runtime source comments.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/nodes/output/output_node.rs`
- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/project_runtime/core_project_runtime.rs`
- `lp-core/lpc-model/src/nodes/output/output_def.rs`

Expected changes:

- Ensure `OutputDefView` is generated, exported, and tested.
- Update all `ShaderNode::new` and `FixtureNode::new` call sites.
- Search for runtime-node direct config usage and remove where the previous
  phases made it unnecessary.
- Keep loader-side direct def reads for:
  - shader source file loading;
  - fixture output sink resolution;
  - output sink service registration.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine project_runtime
cargo test -p lpc-engine nodes::output
cargo test -p lpc-model output
```
