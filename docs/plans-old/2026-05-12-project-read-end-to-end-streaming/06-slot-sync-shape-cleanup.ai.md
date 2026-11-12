# Phase 6: Slot Sync Shape Cleanup

## Scope Of Phase

Clean up slot sync types so node slot roots do not carry or imply a full shape
registry sync.

In scope:

- Replace `WireSlotFullSync { registry: Option<_>, roots }` WIP with separate
  types for full slot sync and roots-only snapshots.
- Update `NodeReadResult.slots` to use the roots-only type.
- Update `SlotMirrorView` and project-read apply logic.
- Keep mockup/standalone full sync using the full type.

Out of scope:

- Registry diffs.
- New slot watch protocol.
- Changing slot data semantics.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep type names honest; avoid optional fields that change the semantic meaning
  of a type.
- Put helpers lower in files when that improves readability.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-wire/src/slot/sync.rs`
- `lp-core/lpc-wire/src/slot/access_sync.rs`
- `lp-core/lpc-wire/src/messages/project_read/node_read.rs`
- `lp-core/lpc-engine/src/engine/project_read_nodes.rs`
- `lp-core/lpc-view/src/slot/mirror.rs`
- `lp-core/lpc-view/src/project/apply_project_read.rs`
- `lp-core/lpc-slot-mockup`
- `lp-core/lpc-wire/tests/source_slot_sync.rs`

Expected changes:

- Introduce:

```rust
WireSlotFullSync {
    registry: SlotShapeRegistrySnapshot,
    roots: Vec<WireSlotRootSnapshot>,
}

WireSlotRootsSnapshot {
    roots: Vec<WireSlotRootSnapshot>,
}
```

- `build_slot_full_sync` returns `WireSlotFullSync`.
- Add a helper to build roots-only snapshots when a registry has already been
  sent through `ProjectReadResult::Shapes`.
- `NodeReadResult.slots` becomes `Option<WireSlotRootsSnapshot>`.
- `SlotMirrorView` gets an apply method for roots-only snapshots that preserves
  the existing registry.

Tests:

- Existing full sync tests still pass.
- Project read apply can receive shapes first, then roots-only node slots.
- Roots-only apply fails clearly if shapes are missing.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-wire
cargo test -p lpc-view
cargo test -p lpc-slot-mockup
cargo test -p lpc-engine default_debug_read_returns_shapes_nodes_and_resource_summaries
```

