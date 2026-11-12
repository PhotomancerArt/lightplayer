# ADR: Authored Bindings Live at Node-Def Roots

- **Status:** Accepted
- **Date:** 2026-07-06
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None


> **Note (2026-07-08):** binding-ref syntax has since changed to
> `bus:<channel>` / `node:<path>#<slot>` and `time.seconds` was renamed
> `time` — see ADR `2026-07-08-binding-ref-syntax-and-channel-naming`.
> Examples below use the older syntax.

## Context

Authored bindings (`BindingDefs`: local slot name → exactly one of
`value`/`source`/`target`) appeared in two places: on every node def root,
and nested inside playlist entries (`entries.<n>.bindings`). Playlist
entries were the only nested case in the model.

The nested case was expensive out of proportion to what it bought:

- The loader special-cased playlists — extracting entry binding sources and
  hoisting them onto the playlist node as deep-path bindings
  (`entries[2].trigger`), constructed via path-string formatting.
- At runtime the playlist node owned the hoisted bindings anyway; the
  nesting existed only in the authoring surface.
- Every upcoming binding UI surface (indicators, binding authoring, the
  bus pane's consumer listings) would have had to handle bindings at
  arbitrary depths for this one consumer.
- Per-entry trigger slots meant N consumption points for what is
  conceptually one channel subscription per playlist.

This shape was built under time pressure for the first working
LightPlayer installation (fyeah sign) and was flagged for cleanup during
the Studio binding/bus design session (2026-07-06).

## Decision

**Authored `bindings` maps exist only at node-def roots.** The norm is
enforced by the model shape, not a validator: no model type other than
node defs carries a `BindingDefs` field.

For the playlist specifically:

- `PlaylistDef` gains the root consumed slot
  `trigger: MapSlot<u32, ControlMessage>` (ByKey merge), moved verbatim
  from `PlaylistEntry`. It binds like any other root slot
  (`"bindings": {"trigger": {"source": "bus#trigger"}}`).
- `PlaylistEntry` loses `bindings` and `trigger`; it gains
  `trigger_ids: OptionSlot<U32ListSlot>` — plain entry config naming the
  trigger message ids (button ids) that start or restart the entry.
  Absent means never triggered. When several entries claim the same id,
  the lowest entry index wins.
- Trigger routing (which entry a message enters) is config; the wire-in
  (which channel feeds the playlist) is a binding. Events-as-values
  semantics (`ControlMessage { id, seq }`, consumer-side seq edge
  detection) and ByKey multi-provider merge (button + radio both writing
  `bus#trigger`) are unchanged.

A supporting change: `Vec<u32>` is representable as a registered slot
value via the `U32List` newtype (`lpc-model/src/slots/u32_list.rs`), and
the `SlotValue` derive can infer `LpType::List(U32)` for `Vec<u32>`
newtype fields.

## Consequences

- Binding discovery is uniform: readers (loader, future effective-binding
  wire surface, Studio indicators/authoring, bus pane) look in exactly one
  place per node.
- The loader's playlist entry-binding special case
  (`playlist_entry_trigger_sources`, `entries[{i}].trigger` path
  construction) is deleted; playlist trigger registration mirrors `time`.
- The playlist resolves one root trigger slot per frame instead of one
  per entry; trigger edge state shrinks to `id → seq`.
- Behavioral delta: an entry used to trigger on *any* message id arriving
  on its bound channel; ids are now explicit. Existing examples are
  unaffected (all buttons use id 1) and behavior is pinned by
  characterization tests (restart, idle return, duplicate-claim,
  unmatched-id).
- Copying an entry between playlists carries its trigger behavior
  (`trigger_ids` travels with the entry).

## Alternatives Considered

- **Generic nested bindings** (any `bindings` map registers against its
  sibling slots): principled, and would also delete the special case, but
  commits every UI surface to arbitrary-depth binding paths forever for a
  single consumer.
- **Playlist-level routing map** (`triggers: MapSlot<u32, u32>`, message
  id → entry index): structurally forbids duplicate claims but splits
  entry behavior away from the entry, reads cryptically in JSON, and
  breaks entry self-containment.
- **`triggered: bool` per entry**: covers the shipped use case but makes
  multi-button → multi-entry a model change later; `trigger_ids` makes the
  bool redundant instead.

## Follow-ups

- "Any id" trigger convention, if a real case materializes (additive).
- Default-binding policy metadata (planned; separate ADR) will let
  `trigger` default to `bus#trigger` so minimal playlists need no
  authored binding.
- The Studio binding/bus roadmap
  (`planning/lp2025/2026-07-06-studio-binding-bus`) builds on this norm
  for indicators, the effective-bindings wire surface, and bus-only
  binding authoring.
