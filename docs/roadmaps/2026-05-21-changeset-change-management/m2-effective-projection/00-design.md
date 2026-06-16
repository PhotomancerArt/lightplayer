# M2 Design — Effective Projection

## Scope

Effective artifact byte reads and effective def projection via `NodeDefView`.
**`lpc-engine` untouched.**

Depends on M1 (`ChangeOverlay`, apply/discard).

## File structure

```
lp-core/lpc-node-registry/src/
├── registry/
│   ├── effective_read.rs          # read_effective_bytes, parse_effective_state
│   └── node_def_registry.rs       # delegate / thin wrappers
├── view/
│   └── node_def_view.rs           # effective get(state)
└── tests/
    └── effective_projection.rs    # D1 view vs committed
```

## Architecture

```text
read_effective_bytes(path, fs)
    │
    ├─ overlay.contains(path)?
    │     ├─ Deleted  → None (parse → error state)
    │     └─ Bytes    → return bytes
    │
    └─ else artifact_path_to_id → store.read_bytes

parse_effective_state(artifact_id, fs, ctx)
    └─ read_effective_bytes(artifact_root_path) → NodeDef::read_toml

NodeDefView::get(id, fs, ctx) -> Option<NodeDefEntry>
    └─ committed entry metadata + effective state (owned clone)
```

### API

```rust
impl NodeDefRegistry {
    /// Bytes for `path` from overlay if present, else committed store/fs.
    pub fn read_effective_bytes(
        &mut self,
        path: &LpPath,
        fs: &dyn LpFs,
    ) -> Result<Option<Vec<u8>>, RegistryError>;

    pub fn view(&self) -> NodeDefView<'_>;
}

impl NodeDefView<'_> {
    /// Effective def entry (overlay ∪ base). Always owned.
    pub fn get(
        &self,
        id: &NodeDefId,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Option<NodeDefEntry>;

    pub fn state(
        &self,
        id: &NodeDefId,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Option<NodeDefState>;
}
```

**Unchanged:** `NodeDefRegistry::get` returns committed `entries` (internal/sync
cache). Callers wanting effective state use `view().get(...)`.

### M2 overlay → def semantics

Only whole-file overlay bytes (M1 `SetBytes` / `Delete`). Replacing
`/clock.toml` bytes replaces the entire parsed tree for all `DefSource` rows on
that artifact until discard.

Slot-level overlay draft (partial TOML merge) is **M4**.

## Tests

| Test | Asserts |
|------|---------|
| `effective_view_differs_after_toml_setbytes` | apply SetBytes on `/clock.toml`; view rate=2, committed rate=1 |
| `effective_view_matches_committed_without_overlay` | load + view.get == committed |
| `discard_restores_effective_view_to_committed` | after discard, view matches committed |
| `effective_deleted_overlay_yields_parse_error` | Delete on loaded `.toml`; view error, committed loaded |

Use `fixtures::load_clock()` + `ParseCtx`.

## Non-goals

- `materialize_source` overlay (M3)
- Slot op overlay merge (M4)
- Commit / SyncResult (M5)
- Effective cache across frames
