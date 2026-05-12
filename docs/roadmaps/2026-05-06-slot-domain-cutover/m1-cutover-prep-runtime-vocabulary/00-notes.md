# M1 Cutover Prep And Runtime Vocabulary Notes

## Scope

Milestone 1 prepares the production vocabulary needed before source defs and runtime nodes become slot roots.

In scope:

- Convert engine demand/runtime produced and consumed slot identity from `ValuePath` to `SlotPath`.
- Clarify `RuntimeProduct` as the payload carried by produced runtime slots.
- Define conventional root names: `source`, `state`, `params`, `output`.
- Add/refine wire/view request types for watching slot roots by node.
- Add minimal slot metadata needed for generic debug UI rendering.
- Audit legacy `Kind`, prop, and detail vocabulary that would otherwise confuse the cutover.

Out of scope:

- Implementing `SlotAccess` for real source defs.
- Runtime state/params/output slot-root exposure from real nodes.
- Client mutation.
- Removing legacy project/node detail responses.
- Full artifact/project mutation.

## User Notes

- This milestone comes from `docs/roadmaps/2026-05-06-slot-domain-cutover/m1-cutover-prep-runtime-vocabulary.md`.
- The next milestone starts with source defs as slot roots, so M1 should make the naming and request vocabulary ready.
- Mutation should remain future work for this roadmap.
- The bridge strategy is acceptable, but cleanup later must be strong.
- Backwards compatibility outside examples/tests is not important.
- Rename aggressively when the domain vocabulary is wrong, but keep the milestone focused.

## Current Code State

### Engine Runtime Identity

- `lp-core/lpc-engine/src/prop/produced_slot_access.rs`
  - `ProducedSlotEntry = (ValuePath, RuntimeProduct, FrameId)`.
  - `ProducedSlotAccess::get`, `iter_changed_since`, and `snapshot` use `ValuePath`.
  - Docs already mark this as transitional.
- `lp-core/lpc-engine/src/resolver/query_key.rs`
  - `QueryKey::ProducedSlot { node, slot: ValuePath }`.
  - `QueryKey::ConsumedSlot { node, slot: ValuePath }`.
- `lp-core/lpc-engine/src/resolver/production.rs`
  - `ProductionSource::ProducedSlot { node, slot: ValuePath }`.
- `lp-core/lpc-engine/src/binding/binding_entry.rs`
  - `BindingSource::ProducedSlot { node, slot: ValuePath }`.
  - `BindingTarget::ConsumedSlot { node, slot: ValuePath }`.
- `lp-core/lpc-engine/src/bus/bus.rs`
  - bus writer identity still stores a `ValuePath`.
- Core nodes use these paths directly:
  - `TextureNode` exposes width/height props.
  - `ShaderNode` exposes `texture`.
  - `FixtureNode` resolves texture dimensions and shader texture output.
- The legacy authored resolver cascade (`resolver.rs`, `ResolverContext`, `SlotResolverCache`) still uses `ValuePath` for prop/default/binding paths. That path is shader/source-binding oriented and should likely stay out of the first runtime identity conversion.

### Runtime Product Payload

- `lp-core/lpc-engine/src/runtime_product/runtime_product.rs`
  - `RuntimeProduct::{Value(LpsValueF32), Render(RenderProductId), Buffer(RuntimeBufferId)}`.
  - `try_value` rejects `LpsValueF32::Texture2D`.
- The name `RuntimeProduct` is serviceable for engine-produced payloads, but it should be documented as payload, not slot identity.
- Resource refs are already available as `ModelValue::Resource` and semantic slot leaves, but produced runtime slots currently return `RuntimeProduct`, not `ModelValue`.

### Wire / View Watch Vocabulary

- `lp-core/lpc-wire/src/project/wire_project_request.rs`
  - `WireProjectRequest::GetChanges` includes `detail_specifier: WireNodeSpecifier`.
  - Resource summary/payload interest is already separate.
- `lp-core/lpc-wire/src/project/wire_node_specifier.rs`
  - `WireNodeSpecifier::{None, All, ByHandles(Vec<NodeId>)}`.
  - This is node-detail vocabulary, not slot-watch vocabulary.
- `lp-core/lpc-view/src/project/project_view.rs`
  - `ProjectView::detail_tracking: BTreeSet<NodeId>`.
  - `watch_detail`, `unwatch_detail`, and `detail_specifier()` feed `WireNodeSpecifier`.
- `lp-cli/src/debug_ui/ui.rs`
  - Tracks selected nodes in `tracked_nodes`.
  - Initial sync requests `WireNodeSpecifier::All`, then uses `ProjectView::detail_specifier()`.
- `lpc-wire::slot` already has full sync, patch, and mutation payloads, but project sync does not yet request or carry watched slot roots.

### Slot Model Metadata

- `lp-core/lpc-model/src/slot/slot_meta.rs`
  - `SlotMeta { label: Option<String>, description: Option<String> }`.
- `SlotValueShape` has editor hints from recent work, including semantic leaves and resources.
- There is no explicit read-only/writable flag or debug visibility/category yet.

### Legacy Vocabulary

- `Kind` is still used in binding registry and bus channel validation.
- `ValuePath` remains correct for nested leaf value access and legacy source-binding/default paths.
- `NodeDetail`, `NodeState`, and `WireNodeSpecifier` are still the production project sync path.

## Open Questions

### Q1. How far should the `SlotPath` conversion go in M1?

Suggested answer: convert the engine demand/runtime path completely:

- `ProducedSlotAccess`
- `QueryKey`
- `ProductionSource`
- `BindingSource::ProducedSlot`
- `BindingTarget::ConsumedSlot`
- `Bus::claim_writer` writer slot path
- core node produced/consumed call sites and tests

Keep legacy authored resolver cascade types on `ValuePath` for now:

- `ResolverContext`
- `resolve_slot`
- `SlotResolverCache`
- `NodePropSpec`
- `NodeInvocation.overrides`

Why: the demand runtime is the path that will become real runtime slot roots. The old authored cascade is a separate legacy path and converting it now would expand M1 into source binding migration.

User answer: yes.

### Q2. What type should identify watched slot roots on the wire?

Suggested answer: add a typed root selector instead of overloading strings:

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

The project request can carry this alongside the legacy detail specifier during the bridge. `AllState` preserves the debug UI's current "all detail" convenience without making it the domain model. `All` is useful for source/debug sync and should request every conventional root that currently exists for every known node.

User answer: yes, an `All` convenience seems useful.

### Q3. Should root names be an enum now or a string wrapper?

Suggested answer: use an enum for the four conventional roots in project watch requests, and convert to stable strings only when building `WireSlotRootSnapshot` names.

Why: M1 is about establishing norms. A plain string here would make typos and root drift too easy. Future dynamic roots can be added later if real nodes need them.

User answer: reasonable.

### Q4. What minimal metadata should M1 add?

Suggested answer: extend `SlotMeta` with only:

- `read_only: bool` or `writable: bool`
- optional `category: SlotMetaCategory` or simple `debug: bool` only if generic UI truly needs it

Keep labels/descriptions as they are. Do not add final product UI concepts yet.

Question: should read-only live on shape metadata (`SlotMeta`) or be a separate access/policy layer attached to watched roots?

User answer: not sure, but metadata seems reasonable.

### Q5. What should happen to `Kind` in M1?

Suggested answer: do not replace `Kind` yet. Audit and document it as a legacy binding/bus type check that will be revisited after runtime slots have shapes.

Why: replacing `Kind` probably wants `SlotShape` / `ModelType` / shader ABI conversion decisions that belong closer to runtime/root exposure, not prep.

User answer: fine.

### Q6. Should `RuntimeProduct` be renamed?

Suggested answer: keep the name for now and clarify docs. It is the engine-produced payload shape; it is not the slot identity and it is not `ModelValue`.

Why: a rename here could be high churn without changing behavior. The important M1 change is `SlotPath` identity and docs.

User answer: yes.

### Q7. Should M1 rename legacy detail types to make the bridge searchable?

Suggested answer: yes. Rename active legacy project/detail types and request fields so new code cannot accidentally treat them as the primary model.

Potential RustRover rename list:

- `lpc_wire::project::WireNodeSpecifier` -> `WireLegacyNodeDetailSpecifier`
- file `wire_node_specifier.rs` -> `wire_legacy_node_detail_specifier.rs`
- `WireProjectRequest::GetChanges.detail_specifier` -> `legacy_detail_specifier`
- `lpc_wire::legacy::ProjectResponse` -> `LegacyProjectResponse`
- `lpc_wire::legacy::SerializableProjectResponse` -> `SerializableLegacyProjectResponse`
- `lpc_wire::legacy::NodeChange` -> `LegacyNodeChange`
- `lpc_wire::legacy::NodeDetail` -> `LegacyNodeDetail`
- `lpc_wire::legacy::SerializableNodeDetail` -> `SerializableLegacyNodeDetail`
- `lpc_wire::legacy::NodeState` -> `LegacyNodeState`
- private helper `NodeStateSerializer` -> `LegacyNodeStateSerializer`
- private helper `SerializableNodeDetailWithFrame` -> `SerializableLegacyNodeDetailWithFrame`
- `ProjectView::detail_tracking` -> `legacy_detail_tracking`
- `ProjectView::watch_detail` -> `watch_legacy_detail`
- `ProjectView::unwatch_detail` -> `unwatch_legacy_detail`
- `ProjectView::detail_specifier` -> `legacy_detail_specifier`
- debug UI `tracked_nodes` -> `legacy_detail_nodes`
- debug UI `all_detail` -> `all_legacy_detail`
- debug UI `tracked_nodes_changed` -> `legacy_detail_nodes_changed`

Names already in `lpc_wire::legacy::nodes::*` such as `ShaderState`, `FixtureState`, `OutputState`, and `TextureState` may be left as-is for now because their module path already says legacy and renaming every state struct would churn node panel code. If desired, they can be renamed in the legacy removal milestone.

Why: M1 is a vocabulary milestone. Having `legacy` in the type and field names will make the bridge visible and later deletion much easier.

User answer: yes, likely useful. User can do RustRover-assisted renames quickly if given a list.

## Suggested Phase Shape

1. Convert engine runtime slot identity to `SlotPath`.
2. Rename active legacy detail/project types and fields.
3. Add wire/view slot watch vocabulary alongside legacy detail specifiers.
4. Add minimal metadata and docs for conventional roots.
5. Audit/quarantine legacy vocabulary and run focused validation.
