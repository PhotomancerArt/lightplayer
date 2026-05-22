# Phase 01 — Change Language Types

**Dispatch:** sub-agent: yes | parallel: -

## Scope of phase

Add serde change vocabulary under `src/change/` per
[`change-language.md`](../../change-language.md).

**In scope:**

- Add `serde = { workspace = true, features = ["derive"] }` to
  `lpc-node-registry/Cargo.toml` (alloc-only, no_std OK).
- Split types one concept per file (see `00-design.md`).
- `ArtifactOp` includes all v1 variants from spec; slot variants are data-only
  in M1 (no apply logic here).
- `ChangeError` enum: `InvalidPath`, `UnknownArtifact`, `UnsupportedOp`, …
- `#[cfg(test)]` serde round-trip tests for `ChangeSet` / `ArtifactChange`.
- Replace `change/mod.rs` stub; export types from `lib.rs`.

**Out of scope:** overlay, registry methods, apply logic.

## Code organization reminders

- Public types at top of each file; `#[cfg(test)] mod tests` at bottom.
- Use `LpPathBuf` from `lpfs` for paths.

## Sub-agent reminders

- Do not commit.
- Do not expand scope into overlay/apply.
- Fix warnings; no allow without reason.

## Implementation details

**Files to create:**

| File | Contents |
|------|----------|
| `change_set.rs` | `ChangeSetId(u64 or String)`, `ChangeSet { id, changes }` |
| `artifact_target.rs` | `ArtifactTarget::Id(ArtifactId)` \| `Path(LpPathBuf)` |
| `artifact_op.rs` | `Delete`, `SetBytes(String)`, slot op structs per spec |
| `artifact_change.rs` | `{ target, ops }` |
| `change_error.rs` | `Display` + error type |

Use `String` for `SetBytes` body (text assets + TOML escape hatch).

**Slot op payloads:** minimal structs with `SlotPath` + value placeholders
(`LpValue` or serde-friendly owned form from `lpc-model` — use existing types
where already serde-enabled).

**lib.rs exports:**

```rust
pub use change::{
    ArtifactChange, ArtifactOp, ArtifactTarget, ChangeError, ChangeSet, ChangeSetId,
};
```

Keep `change` as `pub mod change` or re-export only — match crate style.

## Validate

```bash
cargo test -p lpc-node-registry change::
cargo check -p lpc-node-registry
```
