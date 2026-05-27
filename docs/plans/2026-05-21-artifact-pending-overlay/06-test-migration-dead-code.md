# Phase 6: Test Migration + Delete Dead Overlay Code

## Scope of phase

Remove **`DefDraft`**, **`SlotOverlay`**, **`SlotOverlayEntry`**. Clean deprecated aliases
if safe. Ensure all integration tests assert pending-map semantics where relevant.

**In scope:**

- Delete `edit/def_draft.rs`, `edit/slot_overlay.rs`
- Update `edit/mod.rs`, `lib.rs` — remove dead exports and deprecated aliases for
  `SlotOverlay` / `DefDraft` / `OverlayEntry` / `SlotDraft` if nothing in workspace uses
  them (grep first; keep deprecated types only if external callers exist in same crate tests)
- Update tests that referenced `slot_overlay_*` naming to `overlay_*` where user-facing
- Grep for `DefDraft`, `SlotOverlayEntry`, `SlotOverlay`, `apply_def_draft`

**Out of scope:**

- Roadmap doc updates (optional one-line note in summary only)
- Wire crate

## Code organization reminders

- After deletes, `cargo check -p lpc-node-registry --no-default-features` must pass.
- Tests at bottom of files.

## Sub-agent reminders

- Do **not** commit.
- Do **not** weaken tests to green.

## Implementation details

### Deletions

- `lp-core/lpc-node-registry/src/edit/def_draft.rs`
- `lp-core/lpc-node-registry/src/edit/slot_overlay.rs`

### Export cleanup (`edit/mod.rs`, `lib.rs`)

Remove:

```rust
pub use def_draft::DefDraft;
pub use slot_overlay::{SlotOverlay, SlotOverlayEntry};
```

Remove deprecated type aliases if grep shows no uses outside this crate's deprecated
re-exports in `lib.rs` legacy block.

### Test updates

Review and adjust assertions in:

- `tests/slot_overlay.rs` — rename file optional (not required); update comments
- `tests/overlay_lifecycle.rs`
- `tests/commit_promotion.rs`
- `tests/pending_sync.rs`
- `registry/commit.rs` tests — remove `DefDraft` fixture

Add one test: after two AssignValue same path, introspection shows **one** slot edit (last
value).

### Grep checklist

```bash
rg 'DefDraft|SlotOverlay|SlotOverlayEntry|slot_overlay' lp-core/lpc-node-registry
```

Zero hits in src/ except possibly CHANGELOG — fix all.

## Validate

```bash
cargo test -p lpc-node-registry
cargo check -p lpc-node-registry --no-default-features
```
