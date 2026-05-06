# Phase 1: Runtime SlotPath Identity

## Scope Of Phase

In scope:

- Convert runtime produced/consumed slot identity from `ValuePath` to `SlotPath`.
- Update engine resolver, binding, bus, node, and test support call sites.
- Keep legacy authored resolver/source-binding paths on `ValuePath`.

Out of scope:

- Project wire/view watch vocabulary.
- Source def slot roots.
- Runtime state roots.
- Client mutation.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.
- Do not rename legacy source-binding paths in this phase.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update these runtime-domain files from `ValuePath` to `SlotPath`:

- `lp-core/lpc-engine/src/prop/produced_slot_access.rs`
- `lp-core/lpc-engine/src/resolver/query_key.rs`
- `lp-core/lpc-engine/src/resolver/production.rs`
- `lp-core/lpc-engine/src/resolver/resolve_session.rs`
- `lp-core/lpc-engine/src/resolver/resolver_cache.rs`
- `lp-core/lpc-engine/src/resolver/resolve_trace.rs`
- `lp-core/lpc-engine/src/binding/binding_entry.rs`
- `lp-core/lpc-engine/src/binding/binding_registry.rs`
- `lp-core/lpc-engine/src/bus/bus.rs`
- `lp-core/lpc-engine/src/bus/channel_entry.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/engine/test_support.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- core node files under `lp-core/lpc-engine/src/nodes/core/`.

Expected changes:

- `ProducedSlotEntry = (SlotPath, RuntimeProduct, FrameId)`.
- `ProducedSlotAccess::get(&SlotPath)`.
- `QueryKey::{ProducedSlot, ConsumedSlot}` store `SlotPath`.
- `BindingSource::ProducedSlot` and `BindingTarget::ConsumedSlot` store `SlotPath`.
- `ProductionSource::ProducedSlot` stores `SlotPath`.
- Runtime node helper functions such as `shader_texture_output_path()` and `texture_dimension_query_targets()` return `SlotPath`.
- Tests use `SlotPath::parse(...)` helpers for runtime slot endpoints.

Do not change:

- `lp-core/lpc-engine/src/resolver/resolver.rs`
- `lp-core/lpc-engine/src/resolver/resolver_context.rs`
- `lp-core/lpc-engine/src/resolver/slot_resolver_cache.rs`
- `lpc_model::NodePropSpec`
- `lpc_source::NodeInvocation.overrides`

Those remain legacy authored `ValuePath` paths.

## Validate

```bash
cargo fmt -p lpc-engine
cargo test -p lpc-engine
```

