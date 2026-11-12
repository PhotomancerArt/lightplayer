# Phase 01 — Registry types + ParseCtx

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

Add core registry types and wire module exports. No walker, no store integration
yet.

**In scope:**

- `NodeDefId` (opaque handle, mirror `ArtifactId` pattern)
- `DefSource { artifact_id, path: SlotPath }`
- `NodeDefState` (`Loaded` / `ParseError` / `ValidationError` stub)
- `NodeDefEntry` (source, state, `last_seen_revision: Revision`)
- `NodeDefUpdates { added, changed, removed: BTreeSet<NodeDefId> }`
- `RegistryError`
- `ParseCtx<'a> { shapes: &'a SlotShapeRegistry }`
- `registry/mod.rs` re-exports; update `lib.rs` to pub-export registry types

**Out of scope:** Walker, registry impl, view, tests beyond type smoke if needed.

## Code Organization Reminders

- One concept per file under `registry/` (match M1 `artifact/` layout).
- Public types / re-exports at top of `mod.rs`; impls in dedicated files.
- Tests at bottom of files only if trivial; gate tests live in phase 05.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** edit `lpc-engine`.
- Report deviations.

## Implementation Details

### `node_def_id.rs`

Mirror `artifact/artifact_id.rs`:

```rust
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeDefId(u32);

impl NodeDefId {
    pub fn from_raw(raw: u32) -> Self;
    pub fn raw(self) -> u32;
}
```

### `def_source.rs`

```rust
pub struct DefSource {
    pub artifact_id: ArtifactId,
    pub path: SlotPath,
}
```

Implement `Eq`, `Hash`, `Ord` (derive or manual — must be stable map key).

### `node_def_state.rs`

```rust
pub enum NodeDefState {
    Loaded(NodeDef),
    ParseError(NodeDefParseError),
    ValidationError(ValidationErrorPlaceholder), // or unit/newtype stub
}
```

Add accessor helpers: `is_loaded()`, `kind() -> Option<NodeKind>` (from loaded
def only).

`ValidationError` is a placeholder enum/struct — document with `// M2: unused`
comment; no semantic validator yet.

### `node_def_entry.rs`

```rust
pub struct NodeDefEntry {
    pub id: NodeDefId,
    pub source: DefSource,
    pub state: NodeDefState,
    pub last_seen_revision: Revision,
}
```

### `node_def_updates.rs`

```rust
#[derive(Default)]
pub struct NodeDefUpdates {
    pub added: BTreeSet<NodeDefId>,
    pub changed: BTreeSet<NodeDefId>,
    pub removed: BTreeSet<NodeDefId>,
}

impl NodeDefUpdates {
    pub fn is_empty(&self) -> bool;
    pub fn merge(&mut self, other: Self); // optional helper for tests
}
```

### `registry_error.rs`

Cover at minimum:

- Locator/path resolution failure
- Duplicate `DefSource` registration
- Unknown `NodeDefId` lookup
- Wrap `ArtifactError` from store ops

### `parse_ctx.rs`

```rust
pub struct ParseCtx<'a> {
    pub shapes: &'a SlotShapeRegistry,
}
```

### `lib.rs`

```rust
pub mod registry;
pub use registry::{
    DefSource, NodeDefEntry, NodeDefId, NodeDefRegistry, NodeDefState,
    NodeDefUpdates, ParseCtx, RegistryError,
};
```

`NodeDefRegistry` can be an empty struct stub in `node_def_registry.rs` for now
(phase 03 fills impl).

## Validate

```bash
cargo check -p lpc-node-registry
cargo clippy -p lpc-node-registry --all-targets -- -D warnings
```
