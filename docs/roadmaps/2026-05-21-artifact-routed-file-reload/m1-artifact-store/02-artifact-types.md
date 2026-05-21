# Phase 02 — Artifact types

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

Implement artifact **types** under `lpc-node-registry/src/artifact/` (no store logic
yet beyond what tests need for type checking):

- `ArtifactId`
- `ArtifactLocation` (`File` only) + `try_from_locator`
- `ArtifactError`
- `ArtifactReadState`
- `ArtifactReadFailure`
- `ArtifactEntry`

Export all public types from `artifact/mod.rs`.

**Out of scope:** `ArtifactStore` methods (phase 03). Do not touch `lpc-engine`.

## Code Organization Reminders

- Granular files per type; `mod.rs` re-exports.
- Unit tests for `try_from_locator` at bottom of `artifact_location.rs`.
- Helpers at bottom of files.

## Sub-agent Reminders

- Do **not** commit.
- Stay in scope — types only.
- Fix warnings; no `#[allow]` without reason.
- Report back: files added, tests run.

## Implementation Details

### `artifact_id.rs`

Mirror engine pattern:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArtifactId { handle: u32 }
```

`from_raw` / `handle()` — `from_raw` pub(crate) until store needs it, or pub(crate) in store module only.

### `artifact_location.rs`

```rust
#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum ArtifactLocation {
    File(LpPathBuf),
}
```

- `file(path) -> Self`
- `try_from_locator(loc: &ArtifactLocator) -> Result<Self, ArtifactError>`
  - `ArtifactLocator::Path(p)` → `File(p.clone())`
  - `ArtifactLocator::Lib(_)` → `ArtifactError::Resolution("library artifact references are not supported yet")`

Use `lpc_model::{ArtifactLocator, LpPathBuf}`.

Port ordering tests from `lpc-engine/src/artifact/artifact_location.rs` (file-only subset).

### `artifact_error.rs`

```rust
pub enum ArtifactError {
    UnknownHandle { handle: u32 },
    InvalidRelease { handle: u32 },
    Resolution(String),
    Read(String),
}
```

No `Load`/`Prepare` — freshness store uses `Read` for transient read failures.

### `artifact_read_state.rs`

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactReadState {
    Unread,
    ReadOk,
    Failed(ArtifactReadFailure),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactReadFailure {
    Deleted,
    NotFound,
    Io { message: String },
    InvalidPath { message: String },
}
```

Add `ArtifactReadFailure::from_fs_error(FsError) -> Self` helper mapping
`FsError::NotFound` → `NotFound`, `Filesystem` → `Io`, `InvalidPath` →
`InvalidPath`.

### `artifact_entry.rs`

```rust
pub struct ArtifactEntry {
    pub id: ArtifactId,
    pub location: ArtifactLocation,
    pub refcount: u32,
    pub revision: Revision,
    pub read_state: ArtifactReadState,
}
```

Use `lpc_model::Revision`.

### Tests

In `artifact_location.rs`:

- Path locator resolves to `File`.
- Lib locator returns `Resolution` error.

## Validate

```bash
cargo test -p lpc-node-registry
cargo clippy -p lpc-node-registry --all-targets -- -D warnings
```
