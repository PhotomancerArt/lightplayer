# Phase 4 - Consumed Slot Default Resolution

## Scope Of Phase

Make unbound consumed slots resolve from authored node definition slots.

In scope:

- Add artifact-store lookup for `NodeDefHandle`.
- Change `EngineResolveHost::produce(QueryKey::ConsumedSlot { .. })` so it:
  - does not tick the node
  - reads the node entry's `NodeDefHandle`
  - resolves the artifact root `NodeDef`
  - looks up the requested `SlotPath`
  - returns `ProductionSource::Default`
- Keep produced-slot behavior unchanged.
- Add resolver/engine tests for authored defaults and binding overrides.

Out of scope:

- Typed `SlotView` API. That is Phase 5.
- Inline def paths beyond returning a clear unsupported error.
- Any UI or wire sync work.

## Code Organization Reminders

- Keep artifact/slot lookup helper code close to the engine host if it is
  engine-specific.
- If a helper becomes a named concept, split it into its own file.
- Tests stay at the bottom of files or in targeted integration tests.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not weaken resolver tests.
- If borrow issues appear around `EngineResolveHost`, report the smallest
  failing shape instead of redesigning the session.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/resolver/resolve_session.rs`
- `lp-core/lpc-engine/src/resolver/production.rs`
- `lp-core/lpc-engine/src/node/node_entry.rs`
- `lp-core/lpc-model/src/slot/slot_lookup.rs`

Expected behavior:

- `ConsumedSlot` still checks bindings in `EngineSession`.
- `EngineResolveHost` is only called for consumed slots when no binding exists.
- Fallback reads authored def slots, not runtime state.
- Fallback requires a leaf value. Non-value slot paths should produce a clear
  `UnresolvedConsumedSlot` or developer-facing error.

Tests to add:

- Unbound `ConsumedSlot { node: texture, slot: "size" }` resolves from
  `TextureDef`.
- A binding targeting the same consumed slot wins over the authored default.
- Produced slot tests continue to tick runtime nodes and read runtime state.

## Validate

```bash
cargo test -p lpc-engine resolver
cargo test -p lpc-engine engine
```

