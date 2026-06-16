# Phase 03 — Apply + Discard Lifecycle

**Dispatch:** sub-agent: yes | parallel: -

## Scope of phase

Wire **apply** and **discard** from registry to overlay; implement target
resolution and M1 op semantics.

**In scope:**

- `change/apply.rs` — `apply_change(overlay, registry, change)` resolving targets:
  - `Path(p)` — require absolute; implicit create overlay entry
  - `Id(id)` — resolve via `NodeDefRegistry` internal `artifact_root_path`
- Op handling: `SetBytes`, `Delete` only; other ops → `ChangeError::UnsupportedOp`
- Multiple ops in one `ArtifactChange` applied in order
- Registry methods:
  - `apply_change(&mut self, change: &ArtifactChange) -> Result<(), ChangeError>`
  - `apply_changeset(&mut self, cs: &ChangeSet) -> Result<(), ChangeError>`
  - `discard_overlay(&mut self)`
- Integration tests in `tests/overlay_lifecycle.rs`:
  - **D1** — load project, snapshot `entries` (ids + states), apply `SetBytes` to
    new path `/pending.glsl`, assert overlay contains path, entries unchanged
  - **D3** — after D1, discard, overlay empty, entries still match snapshot
  - implicit create on path not in store
  - relative path rejected

**Out of scope:** effective bytes read (M2), commit (M5).

## Sub-agent reminders

- Do not commit.
- Do not re-parse defs on apply.
- Reuse `harness/fixtures` where possible.

## Implementation details

**D1 test sketch:**

```rust
let snapshot = registry.entries_snapshot_for_test(); // or clone key fields
registry.apply_change(&ArtifactChange {
    target: ArtifactTarget::Path(LpPathBuf::from("/pending.glsl")),
    ops: vec![ArtifactOp::SetBytes("void main() {}".into())],
})?;
assert!(registry.overlay_contains_path(LpPath::new("/pending.glsl")));
assert_entries_unchanged(&registry, &snapshot);
```

If no test snapshot helper exists, compare `entries.len()`, root `NodeDefState`,
and `source_index` len — avoid brittle full clone unless easy.

**ChangeSet batch:** `apply_changeset` applies each `ArtifactChange` in vec order;
first error aborts (all-or-nothing apply).

## Validate

```bash
cargo test -p lpc-node-registry
cargo test -p lpc-node-registry --test overlay_lifecycle
cargo test -p lpc-node-registry --test fs_change_semantics
```
