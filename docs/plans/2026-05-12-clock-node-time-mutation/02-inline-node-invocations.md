# Phase 2: Inline Node Invocations

## Scope Of Phase

Allow project nodes to instantiate either artifact-backed or inline node definitions.

In scope:

- Refactor `NodeInvocation` to support artifact and inline forms.
- Preserve existing artifact TOML syntax.
- Add inline TOML syntax for normal node defs.
- Update `ProjectLoader` to load inline defs from project data.
- Add focused tests for artifact and inline nodes.

Out of scope:

- Artifact-plus-local-field merge semantics.
- Inline project nodes beyond children of the project root.
- Persisting inline defs back to disk.

## Code Organization Reminders

- Keep `NodeInvocation` docs clear: invocation is parent-owned use-site data.
- Avoid putting large serde helper structs in `mod.rs`; keep them in `node_invocation.rs` unless they become reusable concepts.
- Tests stay at file bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/node/node_invocation.rs`
- `lp-core/lpc-model/src/nodes/node_def.rs`
- `lp-core/lpc-model/src/nodes/project/project_def.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs`

Suggested Rust shape:

```rust
pub enum NodeInvocation {
    Artifact { artifact: ArtifactPathSlot },
    Inline { def: NodeDef },
}
```

Public helpers:

- `NodeInvocation::new(ArtifactLocator)` remains available for existing tests.
- `NodeInvocation::inline(NodeDef)` may be added.
- `artifact_locator()` should return `Option`/`Result<Option<_>, _>` or be replaced with clearer helpers.

TOML forms:

```toml
[nodes.shader]
artifact = "./shader.toml"
```

```toml
[nodes.clock]
kind = "clock"

[nodes.clock.controls]
running = true
rate = 1.0
scrub_offset_seconds = 0.0
```

Implementation notes:

- Use serde untagged or custom deserialize to distinguish `artifact` from `kind`.
- Inline node definitions should parse through the same `NodeDef` kind dispatch as artifacts.
- `ProjectLoader` should create a synthetic artifact/def handle for inline defs. Keep the artifact location diagnostic-friendly, such as `inline:/nodes/clock` or another internal location if the existing `ArtifactLocation` supports it. If not, use a path-like synthetic location in the project artifact namespace and document it.
- `LoadedNode` currently stores `artifact_path`; rename or generalize if inline defs make `artifact_path` inaccurate.

Tests:

- Existing artifact invocation tests still pass.
- `ProjectDef` parses inline clock-shaped node once Clock exists, or use an existing inline node def such as output for this phase.
- `ProjectLoader` loads an inline output or placeholder-capable node into the tree.
- Artifact path resolution for existing examples remains unchanged.

## Validate

```bash
cargo fmt
cargo test -p lpc-model
cargo test -p lpc-engine project_loader
```
