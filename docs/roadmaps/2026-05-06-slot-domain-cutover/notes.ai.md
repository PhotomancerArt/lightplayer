# Slot Domain Cutover Notes

## Scope

This roadmap covers the cutover from the current mixed legacy/domain stack to the slot-domain model across:

- Source model loaded from node TOML artifacts.
- Engine runtime node model and resolver access.
- Wire sync for node/source/config/state/params/resource references.
- Client/view mirror and debug egui UI.
- Focused metadata needed for generic UI rendering.

The goal is not to add new node types. The goal is to make the existing project/output/texture/shader/fixture nodes speak the new domain language end to end.

This effort intentionally replaces old vocabulary where it is wrong. Backwards compatibility outside examples/tests is not a concern.

## User Notes

- The old roadmap is now mostly defunct; create a fresh roadmap for the cutover.
- The pieces are hard to decouple because source, engine, wire, and UI all meet at the domain model boundary.
- The UI can remain dev/throwaway egui, but it must demonstrate that the generic slot system works.
- Prefer generic UI as much as practical.
- Detail tracking likely becomes slot watching. For now `state` can be the convention for watched runtime state.
- Resource payloads should probably be opt-in. Resource metadata/skeletons should sync without raw texture/buffer bytes until the UI asks for them.
- There may be prep work worth doing before the main cutover.

## Current Code State

### Source

- `lpc-source` has real node definitions:
  - `ProjectDef` with `nodes: BTreeMap<NodeName, NodeInvocation>`.
  - `ShaderDef` with `glsl_path`, `texture_loc`, `render_order`, `glsl_opts`.
  - `FixtureDef` with output/texture refs, `MappingConfig`, color order, transform, brightness, gamma.
  - `OutputDef` as a GPIO-strip enum with optional driver options.
  - `TextureDef` with width/height.
- These definitions implement `NodeDef`, but they do not yet implement `SlotAccess` / `StaticSlotAccess`.
- Source config is still cloned/downcast in view compatibility paths.
- Shader authored params are not yet represented in the real source model; the mockup has the intended `param_defs` pressure case.

### Model

- `lpc-model::slot` now has the real foundation:
  - `SlotData`, `SlotShape`, `SlotShapeRegistry`, `SlotAccess`, `StaticSlotAccess`.
  - Typed wrappers such as `SlotValue<T>` and `SlotMap<K, V>`.
  - `SlotRef`, `ValueRef`, `SlotOwner`, `SlotPath`.
  - `SlotShapeBuilder` helpers.
  - Semantic leaf/editor hints including resource refs.
- `SlotRef` is owner + slot path only. `ValueRef` adds a nested `ValuePath`.
- `ValuePath` still exists for nested value access and transitional resolver paths.
- `Kind` / old prop vocabulary still exists in legacy resolver/bus paths.

### Engine

- `Node` currently exposes:
  - `produced() -> &dyn ProducedSlotAccess`
  - `runtime_state() -> &dyn RuntimeStateAccess`
  - legacy projection hooks such as `fixture_projection_info()` and `shader_projection_wire()`.
- `ProducedSlotAccess` still uses `ValuePath`, with docs marking that as transitional.
- Resolver/binding/bus code still uses `ValuePath`, `Kind`, and `RuntimeProduct`.
- Runtime nodes (`ShaderNode`, `FixtureNode`, `TextureNode`, `OutputNode`) hold real runtime data but do not expose it as slot roots.
- `CoreProjectRuntime::get_changes()` still builds legacy `ProjectResponse::GetChanges`, including node changes, node details, and resource payload lists.
- Legacy detail projection lives in `project_runtime/detail_projection.rs` and builds typed legacy `NodeState` objects from runtime hooks and compatibility config.

### Wire

- `lpc-wire::slot` now has:
  - `WireSlotFullSync`
  - `WireSlotRootSnapshot`
  - `WireSlotPatch`
  - `WireSlotMutationRequest` / response
  - generic `build_slot_full_sync`, `snapshot_slot_root`, and `collect_slot_diff`.
- Project wire still uses `WireProjectRequest::GetChanges` and legacy `ProjectResponse` for the real app path.
- Resource sync is separate:
  - summaries: `ResourceSummarySpecifier`
  - runtime buffer payloads: `RuntimeBufferPayloadSpecifier`
  - render product payloads: `RenderProductPayloadRequest`
- Resource summaries already support skeleton metadata without bytes.

### View / UI

- `lpc-view::slot::SlotMirrorView` can apply full sync, registry snapshots, patches, and prepare pending set-value mutations.
- `ProjectView` is still legacy-project shaped:
  - node entries contain typed `Box<dyn NodeDef>` config and optional legacy `NodeState`.
  - `detail_tracking: BTreeSet<NodeId>` drives `WireNodeSpecifier`.
  - resource cache is separate.
- `lp-cli` debug UI still renders node-specific panels based on legacy `NodeState`.
- The UI has an `All detail` checkbox and per-node detail toggles.
- Resource payload behavior is already request-based in the transport layer, but the UI does not yet present resource skeletons separately from payload fetching.

### Mockup

- `lpc-slot-mockup` is a useful pressure harness:
  - source/engine/wire/view-shaped modules.
  - dynamic shader params.
  - map key add/remove.
  - enum switches.
  - option changes.
  - full and incremental sync.
  - pending client mutation.
- The mockup is not the production path and should shrink or be deleted after the real cutover proves the same behavior.

## Open Questions

### Q1. What is the first production slice?

Suggested answer: start with source defs as slot roots plus generic client rendering of those source roots. That avoids changing tick/resolver semantics first and gives a concrete UI proof over real TOML-authored node data.

Why: engine state/produced slots are more cross-cutting because they touch resolver, resources, and runtime products.

User answer: yes.

### Q2. What replaces node detail watching?

Suggested answer: introduce explicit slot-root watch requests. Use a convention like root names `source`, `state`, `params`, `output` per node, and let the first production version support watching `state` by node. Keep `All detail` as a debug UI convenience that means "watch each node's conventional state root."

Why: this matches the future model without forcing a perfect subscription language immediately.

User answer: yes.

### Q3. How should resources be watched?

Suggested answer: keep resource summaries always requestable as metadata/skeletons, and add explicit payload interest from the UI for raw bytes. The debug UI should show resource refs and metadata generically, then request bytes only for selected previews.

Why: this preserves low-bandwidth behavior and maps well to existing `ResourceSummarySpecifier` / payload specifiers.

User answer: yes.

### Q4. Should mutation be part of this roadmap?

Suggested answer: include mutation readiness and maybe a small source mutation slice, but do not require full project editing for the main cutover. Full artifact mutation through the message API can be a later roadmap or final optional milestone.

Why: the data model points directly at mutation, but source/engine/wire/view cutover is already large.

User answer: mutation should go to future work. The engine does not yet have the right machinery for mutation, and after this cutover an engine cleanup pass is likely warranted to ready it for a real UI.

### Q5. Do source defs become slots directly, or do they wrap a config field?

Suggested answer: source defs should implement `StaticSlotAccess` directly for now. Avoid forcing an artificial `config` subobject unless it clarifies a node's domain model.

Why: earlier discussion concluded an artifact is a reusable node definition, and defs/configs may not be wholly separate once mutation exists.

User answer: yes.

### Q6. What happens to `ProducedSlotAccess`?

Suggested answer: convert produced/consumed runtime access to `SlotPath` before engine slot roots become primary. Keep `RuntimeProduct` for produced values, but make slot identity `SlotPath`, not `ValuePath`.

Why: this is one of the remaining conceptual mismatches and will become painful once nodes expose real slot trees.

User answer: yes.

### Q7. Do we need more metadata before UI work?

Suggested answer: add only the metadata needed for generic debug rendering:
label, description/help, editor hint, read-only/writable, and possibly visibility/debug category. Avoid building a full design-system vocabulary now.

Why: generic egui needs enough information to avoid nonsense controls, but this is not the final UI.

User answer: yes.

### Q8. Where does real slot sync live in project messages?

Suggested answer: add slot sync payloads alongside the existing project response first, then remove legacy node details after parity. This gives an incremental migration path while still aiming to delete compatibility projection.

Why: source/engine/view migration needs a bridge period unless we do one huge risky replacement.

User answer: yes, with a strong cleanup milestone at the end so the temporary bridge does not become the new permanent mess.

Updated decision after tag `2026-05-07-pre-legacy-remove`: do not bridge the old project messages. The tag/worktree preserve the legacy implementation as reference material. From M2.2 onward, delete the legacy project sync/detail/UI path and rebuild canonical slot-first messages, client view, and debug UI.

## Initial Roadmap Shape

Updated phases:

1. Prep: settle project-level slot watch requests, resource payload selection, root naming, and runtime `SlotPath` cleanup.
2. Source cutover: real node defs implement static slot access and register source shapes.
3. M2.1/M2.x prep: value leaves, inline values, and legacy project sync demolition.
4. M3: rebuild canonical project sync messages around slots, node lifecycle, and resource summaries/payload interest.
5. M4: rebuild project view/client state around `SlotMirrorView`, node index, and resource cache.
6. M5: rebuild generic debug UI over canonical slot sync and opt-in resources.
7. M6: expose runtime node state/params/output as slot roots.
8. M7: cleanup and validation.
