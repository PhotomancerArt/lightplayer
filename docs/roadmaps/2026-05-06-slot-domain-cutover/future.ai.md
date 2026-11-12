## Engine Mutation And Real UI Editing

- **Idea:** Add server-side project/artifact/slot mutation through the message API after the slot-domain cutover.
- **Why not now:** The engine does not yet have the right mutation machinery. The cutover itself is already large and should first make source, runtime, wire, and view speak one slot-domain language.
- **Useful context:** `lpc-wire::slot::WireSlotMutationRequest` and `lpc-view::SlotMirrorView::prepare_set_value` exist from the mockup work, but production engine/source mutation needs a cleanup pass after the cutover.

## Engine Cleanup For UI Readiness

- **Idea:** After legacy detail/state projection is removed, clean up the engine API around node roots, resolver access, resource ownership, and UI-facing sync/mutation boundaries.
- **Why not now:** The cleanup depends on seeing the real shape of source/runtime slot exposure during the cutover.
- **Useful context:** Current friction points include `Node` vs runtime node naming, `ProducedSlotAccess` using `ValuePath`, legacy projection hooks on `Node`, and project sync still returning legacy `ProjectResponse` detail data.
