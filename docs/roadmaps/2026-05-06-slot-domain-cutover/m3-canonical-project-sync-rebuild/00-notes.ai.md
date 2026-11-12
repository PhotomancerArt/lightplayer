# M3 Canonical Project Sync Rebuild Notes

## Scope Of Work

M3 rebuilds project sync after M2.2 removed the legacy project response/detail
path. The goal is a slot-first canonical sync protocol that can sync the real
loaded project tree, source slot roots, slot shape registry snapshots/changes,
and resource summaries/payloads.

In scope:

- Add canonical project sync request/response types in `lpc-wire`.
- Replace `WireProjectRequest::SyncDisabled` with a real project sync request.
- Include node tree deltas in project sync responses.
- Include source slot root full snapshots and incremental patches.
- Include slot shape registry snapshots or changed shape roots needed by slot
  roots/patches.
- Include resource summaries and explicit resource payload responses using the
  existing resource sync primitives.
- Wire `lpa-server` and `lpa-client` through the new response type.
- Add tests for initial full sync, incremental source diffs, registry updates,
  map key removal, resource payload interest, and client application where
  useful.

Out of scope:

- Runtime node state/params/output roots beyond placeholder/watch vocabulary.
- Generic debug UI rebuild.
- Client-driven mutation.
- Source/artifact mutation.
- Reintroducing legacy detail compatibility.
- Firmware render/profile parity; M2.2 left placeholder test targets for the M3
  or later canonical rebuild.

## User Notes

- M2.2 intentionally deleted the old project sync/detail/debug UI path.
- No active external users depend on the legacy protocol.
- Keep resource summaries and explicit payload requests; resource bytes must
  remain opt-in.
- Watch slot roots, not node details.
- Source defs are the first production slot roots; runtime roots are later.
- M3 should keep implementation slices reviewable: wire shape first, engine
  projection second, tests throughout.

## Current Code State

### Wire

- `lp-core/lpc-wire/src/project/wire_project_request.rs` currently only has
  `WireProjectRequest::SyncDisabled`.
- `lp-core/lpc-wire/src/server/api.rs` already keeps the server envelope generic:
  `ServerMsgBody<R>::ProjectRequest { response: R }`.
- `lp-core/lpc-wire/src/lib.rs` temporarily aliases:
  - `WireMessage = Message<NoDomain>`
  - `WireServerMessage = ServerMessage<NoDomain>`
  - `WireServerMsgBody = ServerMsgBody<NoDomain>`
- Tree sync types already exist:
  - `WireTreeDelta`
  - `WireEntryState`
  - `WireChildKind`
  - `WireSlotIndex`
- Slot sync types already exist:
  - `WireSlotFullSync`
  - `WireSlotRootSnapshot`
  - `WireSlotPatch`
  - `WireSlotChange`
  - `build_slot_full_sync`
  - `collect_slot_diff`
- Slot watch vocabulary already exists:
  - `WireSlotRootKind`
  - `WireNodeSlotRoot`
  - `WireSlotWatchSpecifier`
- Resource sync primitives already exist:
  - `ResourceSummarySpecifier`
  - `RuntimeBufferPayloadSpecifier`
  - `RenderProductPayloadRequest`
  - `WireResourceSummary`
  - `WireRuntimeBufferPayload`
  - `WireRenderProductPayload`
- The resource specifier docs still refer to `GetChanges` in a few places and
  should be renamed to canonical sync language.

### Model And Source

- `lpc-model` has `SlotShapeRegistry`, `SlotShapeRegistrySnapshot`, and
  `VersionedSlotShape`.
- `SlotShapeRegistry::snapshot()` currently sends the whole registry.
- `SlotShapeRegistry` tracks `ids_changed_frame` and per-shape
  `changed_frame`, but there is not yet a wire type for registry diffs.
- `lpc-source` has build-generated static source shape bootstrap at
  `lpc_source::slot_shapes`.
- Source defs are `SlotRecord` roots:
  - `ProjectDef`
  - `NodeInvocation`
  - `TextureDef`
  - `ShaderDef`
  - `ShaderParamDef`
  - `OutputDef`
  - `FixtureDef`
  - mapping and GLSL option shapes
- Source defs are real TOML models and can be snapshotted through
  `lpc_wire::build_slot_full_sync`.

### Engine

- `CoreProjectRuntime::project_sync_disabled()` is the current M2.2 placeholder.
- `CoreProjectRuntime` owns:
  - `Engine`
  - `RuntimeServices`
  - `SourceAuthoringIndex`
  - artifact-path-to-node-id index
- `SourceAuthoringIndex` stores `NodeId -> LoadedNodeDef` and
  `NodeId -> LpPathBuf`.
- `LoadedNodeDef` variants wrap concrete source defs:
  - `Texture(TextureDef)`
  - `Shader(ShaderDef)`
  - `Output(OutputDef)`
  - `Fixture(FixtureDef)`
- `ProjectDef` itself is loaded and attached to the runtime root but is not yet
  recorded in `SourceAuthoringIndex`.
- `lpc-engine::tree_deltas_since` already produces `WireTreeDelta`s from the
  runtime tree.
- `project_runtime/resource_projection.rs` still has usable helpers for
  resource summaries and explicit runtime-buffer/render-product payloads.
- There is no project-level slot shape registry in `CoreProjectRuntime` yet.
  M3 needs somewhere to bootstrap source static shapes and any dynamic future
  roots.

### Server And Client

- `lpa-server::handle_project_request` currently validates the project handle
  and returns an explicit disabled-sync error.
- `lpa-client::ProjectGetChangesOptions` survived as resource interest options
  but the name is stale.
- `LpClient::project_sync_disabled` sends `WireProjectRequest::SyncDisabled`.
- Transport types use `WireServerMessage = ServerMessage<NoDomain>` until M3
  introduces a canonical project response body.

### View

- `lpc-view::ProjectView` is now a minimal shell with:
  - `frame_id`
  - minimal `nodes`
  - `slot_watch_roots`
  - `resource_cache`
- `lpc-view::NodeTreeView` can already apply `WireTreeDelta`.
- `lpc-view::SlotMirrorView` can apply `WireSlotFullSync`, registry snapshots,
  and slot patches.
- `ClientResourceCache` can apply resource summaries and payloads.
- M4 is the full `ProjectView` rebuild, but M3 may need enough application
  helpers or integration tests to prove the canonical response is usable.

### Tests And Evidence

- `lpc-source` has source slot root evidence for `examples/basic`.
- `lpc-slot-mockup` has a small server/client sync harness proving slot full
  sync, diffs, dynamic shapes, mutation, and view application concepts.
- Firmware render/profile tests are currently ignored placeholders because the
  legacy sync path was removed.

## Open Questions

### Q1. What Is The M3 Boundary Between Project Sync And ProjectView?

Context: M3 needs to produce canonical sync and prove it is consumable. M4 is
the dedicated `ProjectView` rebuild. Current `ProjectView` is intentionally
minimal, but `NodeTreeView`, `SlotMirrorView`, and `ClientResourceCache` already
exist.

Suggested answer: M3 should add the wire/client/server/engine sync path and
tests that apply responses into `NodeTreeView`, `SlotMirrorView`, and
`ClientResourceCache` directly or through small focused helpers. Keep the
complete `ProjectView` API/design for M4. If `ProjectView` gains anything in
M3, it should be a thin apply method that composes the existing mirrors, not a
full UI-facing model.

### Q2. Should The First Canonical Response Use Full Registry Snapshots Or Registry Diffs?

Context: slot full sync currently includes a full `SlotShapeRegistrySnapshot`.
Incremental slot patches do not carry registry changes. `SlotShapeRegistry`
tracks enough frame data to build diffs later, but there is not yet a wire diff
type for shape additions/removals/replacements.

Suggested answer: M3 should start with a whole registry snapshot on every
project sync response that includes slot data. This is simpler, correct, and
probably small enough for source roots. Add a follow-up note for registry diffs
if bandwidth becomes a problem, especially once dynamic runtime shader param
roots are active.

### Q3. How Should Source Slot Root Names Be Encoded?

Context: `WireSlotRootSnapshot` names roots as strings. The mockup uses strings
like `source.shader` and `engine.shader_node`. Production sync has node ids and
root kinds: `WireNodeSlotRoot { node, root }`.

Suggested answer: M3 should derive stable root names from `WireNodeSlotRoot`,
for example `node:<id>#source`, and keep conversion helpers in one place. This
keeps `SlotMirrorView` unchanged for now while making production roots node-id
scoped. Avoid path-based root names; node paths can change and already exist in
tree deltas.

### Q4. Should The Project Root Have A Source Slot Root?

Context: the runtime root is the `ProjectNode`, and `ProjectDef` is a real
source slot root. The loader currently records child source defs in
`SourceAuthoringIndex` but not the project root `ProjectDef`.

Suggested answer: yes. M3 should record the root `ProjectDef` in source
authoring, or add a parallel project-root source accessor, so the client can
watch the project source root consistently with child node source roots.

### Q5. What Should `WireSlotWatchSpecifier::All` Mean In M3?

Context: runtime roots are not in M3 scope. `All` currently says "all
conventional roots for all nodes", which would imply runtime state/params/output
that M3 cannot provide.

Suggested answer: in M3, support:

- `None`: no slot data.
- `ByRoots`: listed roots, but only `Source` roots are accepted.
- Possibly `AllSource` or rename `All` semantics to avoid overpromising.

The existing `All` variant is too broad for the first canonical sync unless it
is documented as "all currently available roots", which may be surprising.

### Q6. What Should The Canonical Project Request/Response Be Named?

Context: `ProjectGetChangesOptions` and old `GetChanges` wording are stale.
The protocol is not merely "changes"; initial sync sends full state for created
tree entries and watched roots.

Suggested answer: use `WireProjectRequest::Sync(WireProjectSyncRequest)` and
`WireProjectSyncResponse`. Client API can be `project_sync(...)`. The request
should include `since_frame: Option<FrameId>`, slot watch interest, resource
summary interest, runtime-buffer payload interest, and render-product payload
interest.

### Q7. Should Source Defs Be Synced As Full Roots On Initial Watch And Patches After?

Context: `collect_slot_diff` emits patches since a frame, while
`build_slot_full_sync` emits full roots. If a client starts watching a root
after frame N, patches since N are not sufficient because the client has no
base data for that root.

Suggested answer: M3 should include full snapshots for all watched source roots
when `since_frame` is `None` or `0`. For incremental sync, it can initially
assume the watch set is stable and send patches. If the watch set changes,
include full snapshots for newly watched roots. This implies either the request
must tell the server which roots the client already has, or M3 keeps the simpler
rule that clients send `since_frame = 0` when changing watched roots.

### Q8. How Much Resource Payload Integration Belongs In M3?

Context: resource projection helpers survived M2.2 and the UI needs resource
skeletons without bytes unless requested. Runtime roots are later, but resource
summaries/payloads are part of project sync.

Suggested answer: include resource summaries and explicit payload responses in
M3. Keep the resource types as-is except renaming docs away from `GetChanges`.
Do not redesign resources into slots yet.

### Q9. Should Firmware Placeholder Tests Be Restored In M3?

Context: M2.2 replaced firmware scene/profile tests with ignored placeholders
because they depended on legacy detail sync. Restoring them requires canonical
client sync and probably resource payload access for output bytes.

Suggested answer: do not require full firmware render/profile restoration in
M3 unless the canonical response can trivially supply the needed output payload.
Add a small host/server integration test first. Leave firmware restoration as a
later validation task if it pulls in runtime output roots or debug UI work.
