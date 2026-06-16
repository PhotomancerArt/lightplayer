# M1 Design — `lpc-node-registry` + ArtifactStore

## Scope

Bootstrap `lpc-node-registry` and implement a **freshness-only**, **requester-owned**
`ArtifactStore`. Entries exist because a caller acquired them; the filesystem
only **invalidates** (bumps `revision`) entries that are already held.

## Ownership model

```
Engine (M6) ──acquire──► NodeDefRegistry (M2) ──acquire──► ArtifactStore
                              │                              │
                              │         release when done      │
                              └──────────────────────────────┘
```

- **Acquire** resolves an authored specifier to `ArtifactLocation::File(path)`,
  creates or reuses the entry, increments **refcount**, returns **`ArtifactId`**.
- **Release** decrements refcount; at **zero**, entry is **removed** from the store.
- **Fs changes** do not create entries. They only affect paths that already have
  an acquired entry in the store.
- **Bad locator** (e.g. `lib:…` unsupported) → **`ArtifactError::Resolution`**
  at acquire time — no entry created.

This matches production engine refcount semantics without retaining `NodeDef`
payloads.

## File structure

```
lp-core/lpc-node-registry/
├── Cargo.toml
└── src/
    ├── lib.rs                      # crate root, re-exports
    ├── artifact/
    │   ├── mod.rs
    │   ├── artifact_id.rs          # opaque ArtifactId
    │   ├── artifact_location.rs    # File(LpPathBuf) only; try_from_specifier
    │   ├── artifact_error.rs
    │   ├── artifact_read_state.rs  # Unread | ReadOk | Failed(ArtifactReadFailure)
    │   ├── artifact_entry.rs
    │   └── artifact_store.rs       # acquire, release, apply_fs_changes, read_bytes
    ├── registry/mod.rs             # stub — M2
    ├── source/mod.rs               # stub — M3
    ├── change/mod.rs               # stub — M5
    └── view/mod.rs                 # stub — M5
```

Delete: `lp-core/lpc-slot-mockup/` and workspace member entry.

## Conceptual architecture

```
                    acquire(locator) / release(id)
                              │
┌──────────────┐    ┌─────────▼─────────┐    apply_fs_changes([FsChange])
│  Caller      │───►│  ArtifactStore    │◄──────────────────────────────
│  (tests/M2)  │    │                   │
└──────────────┘    │  by_handle        │
                    │  location→handle  │
                    └─────────┬─────────┘
                              │ read_bytes(id, &LpFs)  [transient]
                              ▼
                    Vec<u8> dropped; entry keeps
                    ReadOk / ReadError only
```

### ArtifactEntry (freshness-only)

| Field | Type | Role |
|-------|------|------|
| `id` | `ArtifactId` | Opaque handle |
| `location` | `ArtifactLocation` | Resolved `File(path)` |
| `refcount` | `u32` | Requester ownership |
| `revision` | `Revision` | Monotonic content generation |
| `read_state` | `ArtifactReadState` | Last read / fs-notify outcome (see below) |

No `NodeDef`, no `Vec<u8>` on the entry.

### Read state (structured failures)

```rust
enum ArtifactReadState {
    Unread,
    ReadOk,
    Failed(ArtifactReadFailure),
}

enum ArtifactReadFailure {
    /// FsChange::Delete while entry held — watcher-sourced; no read required.
    Deleted,
    /// read_bytes: file not on disk (never existed, or gone before notify).
    NotFound,
    /// read_bytes: FsError::Filesystem or host I/O.
    Io { message: String },
    /// read_bytes: FsError::InvalidPath.
    InvalidPath { message: String },
}
```

Acquire-time locator errors stay in `ArtifactError::Resolution` — not mixed into
`read_state`.

### Revision bumps

| Event | Effect on matching acquired entry |
|-------|-------------------------------------|
| `FsChange::Modify` | `revision = frame` (or `revision.next()`), `read_state = Unread` |
| `FsChange::Create` | Same as modify (path already held — content replaced) |
| `FsChange::Delete` | Bump `revision`; `read_state = Failed(Deleted)` |
| `read_bytes` Ok | `read_state = ReadOk` (bytes not stored) |
| `read_bytes` Err | Map `FsError` → `Failed(NotFound \| Io \| InvalidPath)` |

`apply_fs_changes` takes a **`Revision` frame** argument (caller-supplied sync
tick), same pattern as engine `acquire_location(..., frame)`.

### Public API (M1)

```rust
impl ArtifactStore {
    pub fn new() -> Self;

    pub fn acquire_location(
        &mut self,
        location: ArtifactLocation,
        frame: Revision,
    ) -> ArtifactId;

    pub fn acquire_specifier(
        &mut self,
        locator: &ArtifactSpecifier,
        frame: Revision,
    ) -> Result<ArtifactId, ArtifactError>;

    pub fn release(&mut self, id: &ArtifactId, frame: Revision) -> Result<(), ArtifactError>;

    pub fn apply_fs_changes(&mut self, changes: &[FsChange], frame: Revision);

    pub fn read_bytes(
        &mut self,
        id: &ArtifactId,
        fs: &dyn LpFs,
    ) -> Result<alloc::vec::Vec<u8>, ArtifactError>;

    pub fn revision(&self, id: &ArtifactId) -> Option<Revision>;
    pub fn entry(&self, id: &ArtifactId) -> Option<&ArtifactEntry>;
}
```

## Main components

- **`ArtifactLocation`** — resolved cache key; M1: `File(LpPathBuf)` only.
- **`ArtifactStore`** — refcounted freshness cache; fs changes are invalidations.
- **`ArtifactReadState`** — separates “content generation” (`revision`) from
  “last read attempt outcome”.
- **Stubs** — `registry`, `source`, `change`, `view` modules declared for roadmap
  layout; empty in M1.

## Validation

```bash
cargo +nightly fmt --all
cargo test -p lpc-node-registry
cargo clippy -p lpc-node-registry --all-targets -- -D warnings
```

## Out of scope

- `NodeDefRegistry`, TOML parse, `SourceFileSlot`, ChangeSet, engine cutover.

## Plan phases (dispatch)

Default model policy: **`composer-2.5-fast`**; **`composer-2-fast`** only for
very simple mechanical work. See `/plan` and `/implement` commands.

| # | Phase | Dispatch |
|---|-------|----------|
| 01 | Bootstrap crate + delete mockup | [sub-agent: yes, model: **composer-2-fast**] |
| 02 | Artifact types | [sub-agent: yes, model: **composer-2.5-fast**] |
| 03 | Store acquire/release + fs changes | [sub-agent: yes, model: **composer-2.5-fast**] |
| 04 | Transient `read_bytes` | [sub-agent: yes, model: **composer-2.5-fast**] |
| 05 | Cleanup + validation + `summary.md` | [sub-agent: **supervised**, model: **composer-2.5-fast**] |

Phases run sequentially (each depends on the previous). Single commit at end
of plan per `/implement`.
