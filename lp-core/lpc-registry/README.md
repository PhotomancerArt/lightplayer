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
- `NotWritable` — the governing `SlotRole` is not writable;
- `TypeMismatch` — an `AssignValue` whose value does not match the leaf type.

Role resolution is shape-only (`lpc-model`'s `resolve_slot_role`), so
edits validate at paths where no data exists yet (missing map entries,
inactive enum variants). `RemoveSlotEdit` is allowed regardless of
writability — it only removes pending overlay state.

The singular `mutate` path applies unconditionally (no validation); it has no
wire-facing caller and any new caller must route through the same validation
(see the follow-ups in `docs/adr/2026-07-04-studio-editing-model.md`).

## Node Authoring Operations

`create_node` / `remove_node` (`registry/node_authoring.rs`) are the
dedicated node-lifecycle operations behind the `CreateNode` / `RemoveNode`
wire commands (`docs/adr/2026-07-27-node-authoring-operations.md`). They are
the sanctioned path around the `nodes` map's `Fixed` role: generic slot
gestures on the map stay rejected, while the ops validate everything up
front and then act atomically, staging through the crate-private
`ProjectRegistry::stage_dedicated_op`.

- `create_node` **commits immediately**: it writes asset and def files
  through the injected `LpFs`, rewrites the attach site's **base** file with
  the canonical writer (pending overlay edits ride above, never baked in),
  and re-derives through the same refresh path fs events use. The attach
  site is a `NodeAttachSite`: the project `nodes` map (policy bypass) or any
  writable `NodeInvocationSlot` path such as a playlist entry.
- `remove_node` **stages in the overlay**: an entry `Remove` at the site,
  `Delete` on the def and every asset exclusively referenced by the removed
  subtree (computed by inventory diff, so shared artifacts survive), and a
  recursive sweep of the subtree's pending overlay entries — an orphaned
  overlay would otherwise abort `commit_overlay` mid-write
  (`CommitError::Projection`). Reverting a staged removal needs only
  existing ops (`RemoveSlotEdit` at the site + `ClearArtifact` per staged
  delete).

## Commit Filtering (Debug vs Persisted)

`commit_overlay` materializes persisted edits into node-def artifacts and
**retains transient overlay entries** instead of clearing the overlay
wholesale: entries whose resolved persistence is `Transient` — Debug-role
fields, plus anything produced (`SlotRoleResolution::persistence`) — survive
the commit and keep applying to the effective inventory. That classifier is
the one the studio also uses, so client and server cannot disagree about
whether an edit is a Debug override; an edit whose path resolves in no shape
takes the shared `SlotPersistence::for_unresolved_edit` rule (Setting) and
drops. Belt-and-braces, the JSON slot writer in `lpc-model` also omits Debug
and produced fields, so no transient value can appear in written def bytes
regardless of caller. An only-Debug commit changes no overlay content and
does not bump the overlay revision.

The editing model (why dirty state derives from the overlay, revision gating,
the client edit buffer) is recorded in
`docs/adr/2026-07-04-studio-editing-model.md`.

## Validation

```bash
cargo check -p lpc-registry
cargo test -p lpc-registry
```
