# Phase 3: Projection Layer + Effective Read

## Scope of phase

Implement **projection**: committed artifact bytes/def + `ArtifactPending` → effective
bytes / `NodeDefState`. Rewire `effective_read.rs`, `NodeDefView`, and
`materialize.rs` asset overlay reads.

**In scope:**

- `registry/projection.rs` — new module
- `registry/effective_read.rs` — delegate to projection
- `registry/mod.rs` — declare `mod projection`
- `source/materialize.rs` — read `AssetPending` from overlay
- `view/node_def_view.rs` — unchanged API, works via effective_read

**Out of scope:**

- Commit (phase 4)
- Public `pending_at` API (phase 5)
- Delete old overlay files (phase 6)
- **Cached effective projection** — v1 folds on each read; see `future.md`. Design
  `projection.rs` so a per-artifact cache can wrap it later without API churn.

## Code organization reminders

- `projection.rs` owns fold logic; effective_read stays thin wrappers.
- Reuse `slot_apply.rs` helpers: `apply_op_to_def`, `parse_def_bytes`, `serialize_slot_draft`.

## Sub-agent reminders

- Do **not** commit.
- Do **not** expand scope.
- Fix failing tests from phase 2; all effective tests must pass this phase.

## Implementation details

### `projection.rs`

Core functions:

```rust
/// Effective raw bytes for an artifact path (for fs-like read).
pub fn project_artifact_bytes(
    committed: Option<&[u8]>,
    pending: Option<&ArtifactPending>,
    ctx: &ParseCtx<'_>,
) -> Result<Option<Vec<u8>>, RegistryError>;

/// Effective NodeDefState at artifact root.
pub fn project_artifact_def(
    committed_state: &NodeDefState,
    pending: Option<&ArtifactPending>,
    ctx: &ParseCtx<'_>,
) -> NodeDefState;

/// Effective def at inline `NodeDefLoc` (slice projected root def).
pub fn project_def_at_loc(
    loc: &NodeDefLoc,
    committed_entry: &NodeDefEntry,
    pending: Option<&ArtifactPending>,
    ctx: &ParseCtx<'_>,
) -> NodeDefState;
```

**Asset pending:**

- `None` → use committed bytes
- `Delete` → `None` / parse error (match current deleted overlay behavior)
- `ReplaceBody(bytes)` → use bytes directly

**Slot pending:**

1. Parse committed bytes → `NodeDef` (or use committed loaded state)
2. For each `(path_key, edit)` in pending.slots (stable iteration order — sort keys for
   determinism): `apply_op_to_def(&mut def, edit, ctx, frame)`
3. Serialize back to bytes if needed, or keep def in memory for state

Use `Revision` from caller frame parameter for apply ops.

**Inline defs:** after projecting artifact root def, use existing
`def_state_at_source` / `collect_invocations` walk (from effective_read) to slice
`loc.path`.

### `effective_read.rs`

Replace `DefDraft` / `SlotOverlayEntry` match arms:

```rust
let pending = self.overlay.pending_at(location);
// read committed bytes from store
let bytes = project_artifact_bytes(...)?;
```

`effective_state(loc)`:

- Get committed entry from `defs`
- Get pending via `loc.artifact`
- If no overlay entry for artifact path → return committed state
- Else `project_def_at_loc`

Remove references to `SlotOverlayEntry::DefDraft`.

### `materialize.rs`

Replace `SlotOverlayEntry` matching with `ArtifactPending`:

- Overlay bytes from `AssetPending::ReplaceBody`
- Delete → `ArtifactReadFailure::Deleted`
- Slot-only pending on shader's **parent toml** does not affect glsl file read (unchanged
  behavior)

### Tests

All must pass:

```bash
cargo test -p lpc-node-registry effective_projection
cargo test -p lpc-node-registry slot_overlay
cargo test -p lpc-node-registry asset_overlay
```

Add projection unit tests in `projection.rs`:

- Committed clock + pending AssignValue → effective rate changed
- Asset replace body → effective bytes
- Asset delete → error/None
- Inline child path projection

## Validate

```bash
cargo test -p lpc-node-registry
```
