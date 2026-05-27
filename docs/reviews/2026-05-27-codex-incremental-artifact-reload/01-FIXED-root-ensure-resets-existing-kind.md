# Root Ensure Resets Existing Kind

- **Severity:** P1
- **Status:** fixed
- **First seen:** 2026-05-27-review.md
- **Last reviewed:** 2026-05-27-review.md
- **Owner:** unassigned

## Finding

`EnsurePresent` on a root variant path always calls `set_slot_variant_default` through `NodeArtifact` before checking whether the artifact is already on that variant. That can discard an existing node def payload when a client sends a variant-qualified path such as `Shader.render_order` against an already-`Shader` artifact.

## Evidence

- `lp-core/lpc-node-registry/src/registry/slot_apply.rs:109` - any first field segment is treated as a possible root variant.
- `lp-core/lpc-node-registry/src/registry/slot_apply.rs:111` - the code calls `set_slot_variant_default` unconditionally once the variant name is valid.
- `lp-core/lpc-node-registry/src/registry/slot_apply.rs:125` - the default-constructed artifact payload is written back to `def`.
- `lp-core/lpc-model/src/slot/slot_mutation.rs:198` - the generic `ensure_slot_present` path correctly guards nested enum changes with `if en.variant() != name.as_str()`, but the root bridge bypasses that guard.

## Impact

A valid edit can accidentally reset unrelated fields in the same artifact. For example, assigning one value through a variant-qualified root path can first replace the whole active root node with its default body, then assign only the requested leaf. Any fields not also present in the edit batch are lost.

## Suggested Fix

Before calling `set_slot_variant_default` in the root bridge, check whether `def.variant_name() == variant.as_str()`. If it already matches, either call generic `ensure_slot_present` on the current `NodeDef` for the tail path or wrap in `NodeArtifact` without resetting the variant.

## Resolution

`apply_ensure_present` now recognizes authored root node variants explicitly, only resets the root def when the requested root variant differs from the current artifact kind, and returns the payload-relative tail path for the following value assignment. Same-kind root-qualified edits therefore operate on the active payload instead of rebuilding the whole artifact.

## Validation

- Added `c1_root_variant_path_preserves_existing_same_kind_payload`, which starts with a non-default `Clock` def, applies `SlotEdit::AssignValue` through a root variant-qualified path, and asserts unrelated payload fields are preserved.
- `cargo fmt --check`: passed
- `cargo check -p lpc-node-registry`: passed
- `cargo test -p lpc-model slot_mutation`: passed
- `cargo test -p lpc-node-registry`: passed

## History

- 2026-05-27: opened by Codex review.
- 2026-05-27: fixed by preserving same-kind root payloads during root variant-qualified ensure.
