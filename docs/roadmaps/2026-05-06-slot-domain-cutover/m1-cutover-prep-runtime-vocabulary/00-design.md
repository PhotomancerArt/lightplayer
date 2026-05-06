# M1 Cutover Prep Runtime Vocabulary Design

## Scope Of Work

This plan prepares the runtime and project-sync vocabulary for the slot-domain cutover.

In scope:

- Convert engine demand/runtime slot identity from `ValuePath` to `SlotPath`.
- Preserve legacy authored resolver/source-binding paths on `ValuePath`.
- Finish legacy detail naming so bridge code is easy to search and delete later.
- Add project wire/view slot watch vocabulary next to legacy detail specifiers.
- Add minimal positive metadata for generic UI editing: `SlotMeta::writable`.
- Document conventional root names and transitional legacy vocabulary.

Out of scope:

- Implementing source defs as slot roots.
- Runtime node state/params/output slot roots.
- Project slot sync payload integration.
- Client mutation.
- Legacy detail removal.

## File Structure

```text
lp-core/lpc-model/src/slot/
  slot_meta.rs
  slot_path.rs

lp-core/lpc-engine/src/
  prop/produced_slot_access.rs
  resolver/query_key.rs
  resolver/production.rs
  binding/binding_entry.rs
  binding/binding_registry.rs
  bus/bus.rs
  bus/channel_entry.rs
  nodes/core/*.rs
  engine/*.rs
  node/*.rs

lp-core/lpc-wire/src/project/
  wire_project_request.rs
  wire_node_specifier.rs          # currently renamed type: LegacyWireNodeSpecifier
  wire_slot_watch_specifier.rs    # new
  mod.rs

lp-core/lpc-view/src/project/
  project_view.rs

lp-cli/src/debug_ui/
  ui.rs
  panels.rs
```

## Architecture Summary

Runtime slot identity uses `SlotPath`. Nested data inside an atomic slot value remains `ValuePath` through `ValueRef`, but runtime produced and consumed endpoints should no longer use value-projection paths as slot identifiers.

Legacy source-binding resolution remains on `ValuePath` for this milestone. That includes `NodePropSpec`, `NodeInvocation.overrides`, `ResolverContext`, `resolve_slot`, and `SlotResolverCache`.

Project sync keeps legacy detail requests during the bridge but introduces explicit slot watch vocabulary:

```rust
pub enum WireSlotRootKind {
    Source,
    State,
    Params,
    Output,
}

pub struct WireNodeSlotRoot {
    pub node: NodeId,
    pub root: WireSlotRootKind,
}

pub enum WireSlotWatchSpecifier {
    None,
    AllState,
    All,
    ByRoots(Vec<WireNodeSlotRoot>),
}
```

`WireProjectRequest::GetChanges` carries both `legacy_detail_specifier` and `slot_watch_specifier`. M1 does not need to make the engine honor `slot_watch_specifier`; that belongs to the project sync bridge milestone.

Generic UI metadata gets one positive editability flag:

```rust
pub struct SlotMeta {
    pub label: Option<String>,
    pub description: Option<String>,
    pub writable: bool,
}
```

Default `writable = false` keeps unknown slots read-only in debug UI until a shape explicitly says otherwise.

## Main Components And Interactions

- `ProducedSlotAccess` returns `ProducedSlotEntry = (SlotPath, RuntimeProduct, FrameId)`.
- `QueryKey::{ProducedSlot, ConsumedSlot}` use `SlotPath`.
- `ProductionSource::ProducedSlot` uses `SlotPath`.
- `BindingSource::ProducedSlot` and `BindingTarget::ConsumedSlot` use `SlotPath`.
- `Bus::claim_writer` and `ChannelEntry::writer` use `SlotPath`.
- Core node helper functions return `SlotPath`.
- Existing tests should parse slot paths with `SlotPath::parse(...)`.
- Legacy project detail names should keep `legacy` in type, method, and field names.
- `WireSlotWatchSpecifier` should round-trip through serde and default to `None`.

