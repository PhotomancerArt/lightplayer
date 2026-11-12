# Phase 03 — Engine + Registry Consumers

**Dispatch:** sub-agent: yes | parallel: - | **Depends on:** 02

## Scope of phase

Migrate **`lpc-node-registry`** and **`lpc-engine`** off `NodeDefRef` / old
invocation struct field access. Update TOML in unit tests embedded in these crates.

**In scope:**

- `lpc-node-registry/src/registry/def_walker.rs`
- `lpc-node-registry/src/registry/def_shell.rs`
- `lpc-node-registry/src/registry/effective_read.rs`
- `lpc-node-registry/src/registry/node_def_registry.rs` (`register_invocations`, etc.)
- `lpc-engine/src/engine/project_loader.rs`
- Grep both crates: `NodeDefRef`, `NodeInvocation::path`, `.def`, `def_locator`, `inline_def`

**Out of scope:** `VariantSet`, `slot_apply` shortcut removal, examples/, diff generic ops.

## Implementation details

**Registration / walking:**

```rust
match &invocation {
    NodeInvocation::Ref(locator) => { resolve path, acquire artifact ... }
    NodeInvocation::Def(body) => { register inline at DefSource ... }
}
```

**def_shell:** kind stubs for inline children — match `NodeInvocation::Def`.

**effective_read:** inline `def_state_at_source` — paths may shift from
`entries[n].node.def.*` to `entries[n].node.def.*` (still valid if Def variant
uses `def` sub-record) — verify against new slot paths.

**Inline slot paths:** After slotted enum lands, confirm generic path for inline
edits is e.g. `entries[2].node.def.Clock.controls.rate` (not custom router).

Update test TOML strings in:

- `def_walker.rs` tests
- `def_shell.rs` tests
- `fs_change_semantics.rs`, `node_def_registry.rs` tests (project/playlist snippets)
- `lpc-engine` tests referencing `NodeDefRef`

## Validate

```bash
cargo test -p lpc-model
cargo test -p lpc-node-registry
cargo test -p lpc-engine
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

Registry apply/diff may still use old SetSlot heuristics — OK until phases 04–05.
