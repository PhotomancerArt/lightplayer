# Structural Edits Leave Stale Descendants

- **Severity:** P2
- **Status:** fixed
- **First seen:** 2026-05-27-review.md
- **Last reviewed:** 2026-05-27-review.md
- **Owner:** unassigned

## Finding

Overlay upsert identity is now exact `SlotPath` equality only. That is clean for leaf writes, but structural edits such as `Remove { path: entries[0] }` or `EnsurePresent { path: entries[0].node.Shader }` can leave older descendant edits queued underneath the same parent.

## Evidence

- `lp-core/lpc-node-registry/src/edit/artifact_overlay.rs:39` - `upsert_slot` keys the pending edit by the new edit's exact path.
- `lp-core/lpc-node-registry/src/edit/artifact_overlay.rs:43` - replacement only removes an existing edit when `existing.path() == &target`.
- `lp-core/lpc-node-registry/src/registry/slot_apply.rs:57` - pending ops are applied later in stored order, so stale descendants still execute after a parent presence/kind change unless they happen to share the same exact path.

## Impact

A normal editing sequence can produce either surprising final state or a projection error. For example, a pending descendant assignment for a Clock child can survive a later parent edit that changes that child to Shader. If the stale descendant runs after the kind change, the effective projection can become a parse/mutation error; if it runs before, the later structural edit silently discards it. Both are hard for clients to reason about.

## Suggested Fix

Teach `ArtifactEdits::upsert_slot` about ancestor/descendant conflict policy. A parent `Remove` or structural `EnsurePresent` should remove pending descendants under that path. A later descendant `AssignValue` should supersede an ancestor `Remove` when auto-vivification is intended to recreate the path.

## Resolution

`ArtifactEdits::upsert_slot` now keeps exact path identity for ordinary replacement while adding ancestor/descendant conflict cleanup for structural edits. Parent removes and structural ensures clear stale descendants, positive edits clear ancestor removes so auto-vivification can recreate paths, and redundant child removes are skipped when an ancestor remove already covers the same subtree.

## Validation

- Added overlay tests for parent remove after descendant assign, descendant assign after parent remove, enum-kind ensure after stale descendant assign, and a root field ensure regression that keeps root variant ensures intact.
- `cargo fmt --check`: passed
- `cargo check -p lpc-node-registry`: passed
- `cargo test -p lpc-model slot_mutation`: passed
- `cargo test -p lpc-node-registry`: passed

## History

- 2026-05-27: opened by Codex review.
- 2026-05-27: fixed by adding structural ancestor/descendant conflict cleanup to overlay slot upsert.
