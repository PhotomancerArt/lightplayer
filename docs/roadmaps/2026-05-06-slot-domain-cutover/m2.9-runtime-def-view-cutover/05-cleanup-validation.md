# Phase 5: Cleanup Validation

## Scope Of Phase

Finalize the milestone, remove stale code, update notes, and run focused
validation.

In scope:

- Remove unused imports/helpers left by node constructor changes.
- Search for stale comments referring to old direct-config ownership.
- Update roadmap todo with completed items and any follow-up items discovered.
- Write `summary.md`.
- Run final validation.

Out of scope:

- New runtime features beyond prior phases.
- Large refactors discovered during validation.

## Code Organization Reminders

- Prefer deleting dead code over compatibility shims.
- Do not leave commented-out experiments.
- Keep TODOs only if they are explicit future work and are also captured in
  `future.md` or roadmap todo.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Searches to run:

```bash
rg "ShaderNode::new|FixtureNode::new|config: ShaderDef|BindingRegistry|ProducedSlotAccess" lp-core/lpc-engine/src lp-core/lpc-model/src
rg "TODO|stub|temporary|debug" lp-core/lpc-engine/src/nodes lp-core/lpc-model/src/nodes
```

Expected final state:

- `ShaderNode` does not store `ShaderDef`.
- `FixtureNode` does not store scalar config copied from `FixtureDef`.
- Generated def views exist for shader, fixture, output, and texture.
- Existing examples/tests still load.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model
cargo test -p lpc-slot-codegen
cargo test -p lpc-engine
cargo check -p lpc-model --features schema-gen
cargo clippy -p lpc-engine -p lpc-model -p lpc-slot-codegen -p lpc-slot-macros --all-targets -- -D warnings
```
