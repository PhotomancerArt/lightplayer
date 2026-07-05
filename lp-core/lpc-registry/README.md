# lpc-registry

Effective project registry: authored artifacts plus a pending in-memory
overlay, derived into the effective inventory the engine runs.

`ProjectRegistry` owns the artifact store (node defs, assets), the
`ProjectOverlay` of pending slot edits, and the derived effective inventory.
Loads, filesystem refreshes, mutations, and commits all funnel through it and
re-derive the inventory; the engine consumes the result and never sees the
overlay/artifact split.

## Mutation Policy Enforcement

`mutate_batch` is the wire-facing mutation surface and validates every
command against the slot shape **before** applying it, rejecting invalid
commands individually (`MutationRejectionReason`) while the rest of the batch
proceeds:

- `UnknownArtifact` / `UnknownSlotPath` — the target does not resolve;
- `NotWritable` — the governing `SlotPolicy` is not writable;
- `TypeMismatch` — an `AssignValue` whose value does not match the leaf type.

Policy resolution is shape-only (`lpc-model`'s `resolve_slot_policy`), so
edits validate at paths where no data exists yet (missing map entries,
inactive enum variants). `RemoveSlotEdit` is allowed regardless of
writability — it only removes pending overlay state.

The singular `mutate` path applies unconditionally (no validation); it has no
wire-facing caller and any new caller must route through the same validation
(see the follow-ups in `docs/adr/2026-07-04-studio-editing-model.md`).

## Commit Filtering (Transient vs Persisted)

`commit_overlay` materializes persisted edits into node-def artifacts and
**retains transient overlay entries** instead of clearing the overlay
wholesale: entries whose resolved policy persistence is `Transient` survive
the commit and keep applying to the effective inventory. Belt-and-braces, the
JSON slot writer in `lpc-model` also omits transient fields, so no transient
value can appear in written def bytes regardless of caller. An only-transient
commit changes no overlay content and does not bump the overlay revision.

The editing model (why dirty state derives from the overlay, revision gating,
the client edit buffer) is recorded in
`docs/adr/2026-07-04-studio-editing-model.md`.

## Validation

```bash
cargo check -p lpc-registry
cargo test -p lpc-registry
```
