# Phase 05 — Generic Diff + Test TOML

**Dispatch:** sub-agent: yes | parallel: - | **Depends on:** 04

## Scope of phase

Make **`def_diff.rs`** emit generic ops and migrate **all `lpc-node-registry` test
/fixture TOML** to `ref`/`def` wire.

**In scope:**

- `diff/def_diff.rs` — remove `CustomDef`, `invocation_def_value`, kind-at-root special case
- Enum branch → `push_variant_set` not `push_set_slot + String`
- `tests/*.rs`, `harness/fixtures.rs`, `harness/snapshot*.rs` if any
- `tests/project_diff.rs` snapshot TOML in memory

**Out of scope:** `examples/` tree (phase 07).

## Diff changes

| Before | After |
|--------|-------|
| `SlotKind::Enum` → `SetSlot(String)` | `VariantSet { path, variant }` |
| Root kind change special case | `VariantSet(root(), variant)` |
| `SlotKind::CustomDef` + path string | `VariantSet(..., "Ref")` + `SetSlot(...ref, path)` or recurse into Def |

Add `push_variant_set` mirroring `push_set_slot` (apply-then-push for diff simulation).

Remove `classify_slot` → `CustomDef` arm if `NodeInvocation` is normal enum in tree.

## Fixture TOML grep

Replace across `lpc-node-registry`:

```
def = { path =     → ref =
node = { def = { path =   → ref = (flatten)
```

Playlist inline `[entries.N.node.def]` stays for Def variant body.

## Tests

- `project_diff` equivalence still passes
- Add/adjust test: diff project wiring change emits `VariantSet` + `SetSlot`, not CustomDef
- `def_walker` / `def_shell` tests green with new wire

## Validate

```bash
cargo test -p lpc-node-registry
cargo test -p lpc-node-registry --features diff
```
