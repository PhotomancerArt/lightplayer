# Phase 2: Route ProjectLoader Through Registry

## Scope Of Phase

In scope:

- update `ProjectLoader` definition reads to use the engine's
  `SlotShapeRegistry`
- ensure project root and child node artifacts both load through the new
  `NodeDef` SlotCodec TOML reader
- keep current runtime attachment behavior unchanged
- update project-loader tests or add focused coverage if existing tests do not
  exercise the new path

Out of scope:

- changing artifact resolution semantics
- changing tree structure or runtime node attachment order
- switching ProjectBuilder writes
- removing serde from non-definition code

## Code Organization Reminders

- Keep load helpers close to `project_loader.rs`.
- Avoid broad loader rewrites; reshape only enough to pass the registry into
  definition parsing.
- Preserve existing error variants unless a new error is genuinely clearer.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant file:

- `lp-core/lpc-engine/src/engine/project_loader.rs`

Current helpers:

- `load_project_def(root, path)`
- `load_node_def(root, path)`

Likely reshaping:

1. Resolve the project path before constructing the engine, as today.
2. Construct `Engine::with_services(...)` so the static slot registry exists.
3. Read project TOML text and decode it with `runtime.slot_shapes()`.
4. Decode child node TOML with the same registry.

Be careful with borrow boundaries: avoid holding an immutable borrow of
`runtime.slot_shapes()` across later mutable `runtime` operations. Passing the
registry only into short read helper calls should be enough.

## Validate

```bash
cargo test -p lpc-engine project_loader
cargo test -p lpc-engine project_read
```
