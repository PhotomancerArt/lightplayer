# M1 Notes — Change Language + Overlay Lifecycle

## Scope

Introduce v1 change-language **serde types**, **`ChangeOverlay`** on
`NodeDefRegistry`, and **apply / discard** lifecycle. Bootstrap with a single
`ArtifactChange` + `SetBytes` op. Prove **D1** and **D3**.

**Out of scope:** effective projection (M2), full file op semantics (M3), slot
ops (M4), commit (M5).

## Current codebase

```
lp-core/lpc-node-registry/src/
├── registry/node_def_registry.rs   # owns ArtifactStore + entries; no overlay
├── registry/registry_change.rs     # RegistryChange::Fs only
├── change/mod.rs                   # stub comment only
└── view/node_def_view.rs           # passthrough committed entries
```

Parent M1–M4 complete. Tests use `NodeDefRegistry::load_root` + `sync_fs` with
`LpFsMemory` fixtures (`tests/fs_change_semantics.rs`, `harness/fixtures.rs`).

## Resolved design (2026-05-21)

- **Overlay inside `NodeDefRegistry`** — field between store and entries.
- **`ChangeSet`** = `Vec<ArtifactChange>` grouped by artifact path/id.
- **Implicit create** on `ArtifactTarget::Path`.
- **M1 op surface:** `SetBytes` only (shell apply); `Delete` and slot ops stubbed
  or return `ChangeError::UnsupportedOp` until M3/M4.
- **No effective read path in M1** — tests inspect overlay via test-only or public
  introspection helpers (`overlay_has_path`, `overlay_bytes`); M2 wires reads.
- **Serde on types** — `serde` with `alloc` (no_std-compatible); round-trip unit
  tests under `#[cfg(test)]`.

## Open questions

### Q1: Overlay introspection API for tests / client badges

- **Context:** D1/D3 need to observe overlay without M2 projection. Client may
  query `overlay.contains_path` later.
- **Suggested answer:** Public read-only helpers on registry:
  `overlay_is_active()`, `overlay_contains_path(&LpPath)`, optional
  `overlay_entry_state(path)` for tests. No slot-level introspection until M4.

### Q2: Path key normalization

- **Context:** `ArtifactTarget::Path(LpPathBuf)` must match registry
  `artifact_path_to_id` keys (string paths).
- **Suggested answer:** Normalize to absolute path string via `LpPathBuf` as stored
  in registry; reject relative paths in `apply` with `ChangeError::InvalidPath`.

### Q3: `ArtifactTarget::Id` in M1

- **Context:** Change language supports `Id(ArtifactId)` for committed targets.
- **Suggested answer:** Implement resolve `Id → path` via `artifact_root_path` map;
  if unknown id, error. Needed for parity; tests can use `Path` only initially.

### Q4: Multiple ops in one `ArtifactChange`

- **Context:** Language allows `ops: Vec<ArtifactOp>`.
- **Suggested answer:** M1 apply loops ops sequentially on same overlay entry;
  `SetBytes` replaces bytes; unknown ops error.

## User stories (this milestone)

| ID | Story | How |
|----|-------|-----|
| D1 | Apply → pending visible | Overlay has entry; `entries` unchanged |
| D3 | Discard → base unchanged | Overlay empty; `entries` bit-identical |

## Validation baseline

```bash
cargo test -p lpc-node-registry
cargo test -p lpc-node-registry --test fs_change_semantics
```

Must remain green after M1.
