# M1 Design — Change Language + Overlay Lifecycle

## Scope

Add change vocabulary types and path-keyed overlay with apply/discard on
`NodeDefRegistry`. **`lpc-engine` untouched.**

Spec: [`../change-language.md`](../change-language.md).

## File structure

```
lp-core/lpc-node-registry/
├── Cargo.toml                          # + serde (alloc)
├── src/
│   ├── lib.rs                          # re-export change + ChangeError
│   ├── change/
│   │   mod.rs
│   │   change_set.rs                   # ChangeSet, ChangeSetId
│   │   artifact_change.rs              # ArtifactChange
│   │   artifact_target.rs              # ArtifactTarget
│   │   artifact_op.rs                  # ArtifactOp (+ SetBytes, Delete shell)
│   │   change_error.rs                 # ChangeError
│   │   apply.rs                        # apply_change(s) on &mut ChangeOverlay
│   │   overlay.rs                      # ChangeOverlay, OverlayEntry
│   └── registry/
│       node_def_registry.rs            # + overlay field; apply/discard methods
└── tests/
    └── overlay_lifecycle.rs            # D1, D3 (+ serde round-trip in unit tests)
```

## Architecture

```text
apply(ArtifactChange | ChangeSet)
        │
        ▼
ChangeOverlay (path → OverlayEntry)
        │
        ├─ Deleted          (M1: apply Delete sets flag; read in M2/M3)
        └─ Bytes(Vec<u8>)   (M1: SetBytes)

NodeDefRegistry
  store: ArtifactStore      ← unchanged on apply/discard
  overlay: ChangeOverlay    ← mutated on apply/discard
  entries: ...              ← unchanged on apply/discard (D1/D3)
```

### `OverlayEntry` (M1)

```rust
enum OverlayEntry {
    Deleted,
    Bytes(alloc::vec::Vec<u8>),
    // SlotDraft { ... } — M4
}
```

### Apply pipeline

1. Resolve `ArtifactTarget` → absolute `LpPathBuf` key.
2. `overlay.entry_or_insert(path)` — implicit create.
3. For each op: `SetBytes(b)` → `OverlayEntry::Bytes(b)`; `Delete` → `Deleted`.
4. Other op variants → `ChangeError::UnsupportedOp` in M1.

### Discard

`overlay.clear()` — no touch to `store` or `entries`.

### Registry API (public)

```rust
impl NodeDefRegistry {
    pub fn apply_change(&mut self, change: &ArtifactChange) -> Result<(), ChangeError>;
    pub fn apply_changeset(&mut self, changeset: &ChangeSet) -> Result<(), ChangeError>;
    pub fn discard_overlay(&mut self);
    pub fn overlay_active(&self) -> bool;
    pub fn overlay_contains_path(&self, path: &LpPath) -> bool;
}
```

Names may adjust in phase implementation; semantics fixed.

## Types (summary)

See [`../change-language.md`](../change-language.md). M1 implements full enum
shell; only file ops `SetBytes`/`Delete` apply; slot op variants exist for serde
stability but return `UnsupportedOp` if applied.

## Tests

| Test | Story |
|------|-------|
| `serde_roundtrip_changeset` | Types (unit, in change module) |
| `d1_apply_populates_overlay_base_unchanged` | D1 |
| `d3_discard_clears_overlay_entries_unchanged` | D3 |
| `apply_rejects_relative_path` | Path invariant |
| `apply_setbytes_on_unloaded_path` | Implicit create |

Fixtures: empty registry + `load_root` from existing harness (`clock` or
`shader_project`) to prove `entries` stable across apply/discard.

## Non-goals (M1)

- `NodeDefView` effective reads
- `read_effective_bytes`
- Commit / `SyncResult`
- `RegistryChange::ChangeSet`
