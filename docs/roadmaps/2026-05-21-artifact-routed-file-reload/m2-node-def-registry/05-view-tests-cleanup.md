# Phase 05 — NodeDefView stub + gate tests + cleanup

**Dispatch:** [sub-agent: supervised, model: composer-2.5-fast, parallel: -]

## Scope of phase

Add `NodeDefView` stub, complete gate test coverage (including kind change),
cleanup, fmt, clippy, and write `summary.md`.

**In scope:**

- `view/node_def_view.rs` stub
- Gate tests T1–T5 (consolidate/deduplicate with phase 04 tests if needed)
- `lib.rs` exports for view
- `summary.md`
- Remove stray TODOs; ensure warnings clean

**Out of scope:** `lpc-engine` edits, ChangeSet overlay (M5).

## Code Organization Reminders

- View module stays minimal — lookup only.
- Gate tests may live in `node_def_registry.rs` test module or
  `registry/integration_tests.rs` if cleaner; prefer single test module at bottom
  of `node_def_registry.rs` unless file is too large.
- Test helpers at bottom of test module.

## Sub-agent Reminders

- Do **not** commit (supervisor commits after full plan validation).
- Do **not** expand scope into engine or M3.
- Fix warnings; do not suppress lints.
- Report what changed and validation results.

## Implementation Details

### `NodeDefView` stub

```rust
pub struct NodeDefView<'a> {
    registry: &'a NodeDefRegistry,
}

impl<'a> NodeDefView<'a> {
    pub fn new(registry: &'a NodeDefRegistry) -> Self;
    pub fn get(&self, id: &NodeDefId) -> Option<&NodeDefEntry>;
    pub fn state(&self, id: &NodeDefId) -> Option<&NodeDefState>;
}
```

Document in module doc: M5 adds ChangeSet overlay; M6 engine reads defs through
view only.

Update `lib.rs`:

```rust
pub mod view;
pub use view::NodeDefView;
```

### Gate tests T1–T5

| Test | Assert |
|------|--------|
| T1 `leaf_file_edit_marks_root_changed` | Root in `changed`; sets empty |
| T2 `inline_child_edit_does_not_mark_parent_changed` | Child in `changed`; parent not |
| T3 `playlist_entry_add_and_remove` | `added`/`removed` + parent `changed` |
| T4 `path_child_file_edit_isolated` | Child `changed`; playlist not |
| T5 `inline_child_kind_change_marks_child_and_parent_changed` | Both in `changed`; comment documents M6 delete/recreate |

Use concise TOML fixtures; `LpFsMemory` with `/`-rooted paths. Every gate test
follows **`load_root` → (optional fs mutate) → `apply_fs_changes` → `sync`**.

Optional: one test loading a real file from `examples/basic/clock.toml` if
repo-relative path in test is stable (use `include_str!` or env-free path from
manifest dir via `CARGO_MANIFEST_DIR`).

### `summary.md`

Brief bullet summary:

- Types added
- API surface (`load_root`, `sync`, `NodeDefView`)
- Test count
- Kind-change engine contract note for M6

### Cleanup checklist

- [ ] No `TODO` without ticket/milestone reference
- [ ] `cargo +nightly fmt --all`
- [ ] All `lpc-node-registry` tests pass
- [ ] Clippy clean

## Validate

```bash
cargo +nightly fmt --all
cargo test -p lpc-node-registry
cargo clippy -p lpc-node-registry --all-targets -- -D warnings
```

Supervisor after all phases: single commit per `/implement` — not in this phase
unless user requests.
