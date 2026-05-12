# M3.5 Resolver Slot Data And Merge Summary

## What was built

- Added `SlotMerge` as receiver-owned merge policy vocabulary.
- Made resolver `Production` carry owned `SlotData`, while preserving leaf value helpers for existing node code.
- Added shape-aware slot lookup so runtime/authored slot subtrees can be snapshotted with the correct shape.
- Updated engine production to resolve aggregate runtime/authored slots instead of rejecting non-value slot data.
- Added consumed-slot merge handling for `SlotMerge::ByKey`, including bus-provider expansion and deterministic key replacement.
- Added trace events for merge policy selection, merge inputs, and replaced map keys.
- Added resolver tests for direct map binding merge and bus-provider map merge.

## Decisions for future reference

#### Resolver owns answers

- **Decision:** Resolver cache entries own `SlotData` answers for the current frame.
- **Why:** Owned answers are easy to cache, merge, trace, and hand to consumers without borrowing node/artifact internals.
- **Rejected alternatives:** Cache borrowed `SlotDataAccess` views.
- **Revisit when:** Profiling shows aggregate copies are a hot path.

#### Merge policy belongs to receivers

- **Decision:** `SlotMerge` is chosen by the consumed slot, not by each binding.
- **Why:** The receiver defines what multiple inputs mean for that slot.
- **Rejected alternatives:** Binding-local merge strategies.
- **Revisit when:** We need per-source weighting or filtering in addition to receiver-level merge semantics.

#### `by_key` overwrites deterministically

- **Decision:** Later inputs replace earlier inputs for duplicate map keys, and the trace records replacements.
- **Why:** It gives emitter maps a practical default while keeping conflict handling visible.
- **Rejected alternatives:** Error on duplicate keys.
- **Revisit when:** We need stricter conflict policies for specific receivers.
