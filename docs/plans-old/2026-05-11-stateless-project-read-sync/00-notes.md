# Stateless Project Read Sync Notes

## Scope Of Work

Define and implement the first canonical client/server project read path after
the legacy project-sync demolition.

The core protocol direction is:

- one client-driven read operation;
- `since: Option<Revision>` chooses full snapshot versus incremental changes;
- one envelope with typed domain queries/results;
- product probes are explicit diagnostic requests alongside normal project
  data queries;
- no persistent subscriptions or per-client server state;
- domains use a shared LOD vocabulary: ids, summary, detail;
- detail stays domain-specific and explicit, especially for large resources.

In scope for this plan:

- New `lpc-wire` project read request/response vocabulary.
- Shape registry and slot root snapshot messages as first-class read results.
- Node ids/summary/detail query vocabulary.
- Resource ids/summary/detail query vocabulary, with payload bytes requested
  explicitly.
- Product probe vocabulary, so diagnostic materialization is modeled as a
  request-scoped probe rather than resource sync or node state.
- Engine-side helpers to answer initial full snapshot queries from the current
  loaded `Engine`.
- Client/view helpers to apply the first full snapshot into existing mirrors.
- Tests proving stateless fetch-style reads for `examples/basic`.
- A short docs/lp-core section explaining probes once the vocabulary is stable.

Out of scope for this plan unless explicitly pulled in:

- Persistent watches/subscriptions.
- Server-side client session tracking.
- Push updates.
- Client-driven mutation.
- Incremental slot-data diffs beyond enough shape to keep the protocol ready.
- Dynamic shader param shape changes.
- Generic debug UI.
- Full resource payload optimization.

## User Notes

- The server should be stateless with respect to clients.
- The only client "state" that matters to the server is the revision the client
  says it knows about.
- Nothing is push for this client protocol; everything is pull.
- A future push-data concept may exist for inter-server shared-bus style work,
  but that is not this protocol.
- Fetch and poll should be one operation with `since: Option<Revision>`.
- Top-level messages should not be separate per domain; use one read envelope
  with typed domain variants.
- The UI should be node-centric. Artifacts are internal details of authored
  storage.
- The two user-visible paths are node inspection/editing and filesystem
  browsing/editing. If users want the actual files, they use the fs API.
- Basic/local clients should have an easy way to request most useful detail.
- Rich/embedded clients need precise low-bandwidth requests.
- The old wire language does not need compatibility preservation.

## Roadmap Context

The old roadmap at
`docs/roadmaps/2026-05-06-slot-domain-cutover/` is now mostly historical. It
was valuable while the domain model was still forming, but its M3 notes still
refer to watch-style vocabulary and older project-runtime concepts.

Current recommendation:

- Keep the old roadmap as an archive of decisions and completed milestone work.
- Treat `docs/lp-core/` as the living domain guide.
- Use this standalone plan for the canonical stateless project read protocol.
- Consider a fresh roadmap only if the wire/view/UI/mutation work needs to be
  coordinated as several independent future plans. For this immediate work, a
  focused standalone plan is enough.

## Current Code State

### Domain Docs

- `docs/lp-core/overview.md` describes the current core model:
  nodes, slots, values, bindings, resources, and products.
- `docs/lp-core/todo.md` is now the living checklist.
- The todo already points to generic slot sync, wire/view rebuild, resource
  metadata/payload sync, and removal of old watch/detail vocabulary.

### Engine

- `lpc-engine` is organized around the current concepts:
  - `engine/`
  - `node/` and `nodes/`
  - `dataflow/{binding,bus,resolver}/`
  - `product/` and `products/{visual,control}/`
  - `resource/` and `resources/buffer/`
  - `gfx/`
- `Engine` owns:
  - `NodeTree`
  - `ArtifactStore`
  - `SlotShapeRegistry`
  - runtime buffers
  - dataflow resolver
  - services/output flushing
  - revision/frame state
  - artifact path to node id lookup
- `Engine` can expose tree deltas through existing node tree sync helpers.
- `Engine` has authored defs in `ArtifactStore` and `NodeDefHandle`s on
  `NodeEntry`.
- Runtime state slot roots exist for shader, fixture, and texture.
- Output node currently owns/output-flushes a runtime buffer but does not yet
  expose a rich runtime state root.

### Wire

- `lp-core/lpc-wire/src/project/wire_project_request.rs` currently has only
  `WireProjectRequest::SyncDisabled`.
- `ClientRequest::ProjectRequest { handle, request }` and
  `ServerMsgBody::ProjectRequest { response }` already provide the outer
  project-scoped request/response slot.
- Existing tree wire types:
  - `WireTreeDelta`
  - `WireEntryState`
  - `WireChildKind`
  - `WireSlotIndex`
- Existing slot wire types:
  - `WireSlotFullSync`
  - `WireSlotRootSnapshot`
  - `WireSlotPatch`
  - `WireSlotChange`
  - `build_slot_full_sync`
  - `collect_slot_diff`
- Existing resource wire types:
  - `ResourceSummarySpecifier`
  - `RuntimeBufferPayloadSpecifier`
  - `WireResourceSummary`
  - `WireRuntimeBufferPayload`
- Some existing names still imply watch/subscription or old `GetChanges`
  semantics and should be renamed or replaced by read-query vocabulary.
- `WireSlotWatchSpecifier` and `WireNodeSlotRoot` are old watch-shaped
  vocabulary. They should not survive as canonical project-read types.

### View / Client

- `lpc-view::SlotMirrorView` already applies:
  - full slot sync
  - registry snapshots
  - slot patches
  - pending mutation request bookkeeping
- `lpc-view::NodeTreeView` applies tree deltas.
- `lpc-view::ClientResourceCache` applies resource summaries and payloads.
- `lpc-view::ProjectView` is still a minimal shell with old watch-ish
  vocabulary (`slot_watch_roots`), and should not drive the protocol design.
- `lpa-client` still has old resource/request option names and disabled-sync
  calls.

## Current Message Surface Audit

### Keep And Reuse

- `ClientRequest::ProjectRequest { handle, request }`: keep as the generic
  project-scoped request envelope.
- `ServerMsgBody::ProjectRequest { response }`: keep as the matching response
  envelope.
- `WireProjectHandle`: keep.
- `WireTreeDelta`: reuse for node tree ids/summary/detail where it still fits,
  but clean field names that still say `*_frame` or `*_ver`.
- `WireSlotFullSync`, `WireSlotRootSnapshot`, `WireSlotPatch`,
  `WireSlotChange`: reuse as the slot payload vocabulary.
- `WireSlotMutationRequest` / `WireSlotMutationResponse`: keep out of this
  plan unless a compile cleanup forces a rename; mutation is not part of
  read sync.
- `WireResourceSummary` and `WireRuntimeBufferPayload`: reuse initially, but
  wrap them in new read-query/result names rather than keeping old specifier
  vocabulary as the protocol surface.
- `FsRequest` / `FsResponse`: keep. Filesystem probes are a future extension
  to read-time diagnostics, not a replacement for explicit filesystem
  mutation/read APIs.

### Replace Or Rename

- `WireProjectRequest::SyncDisabled`: replace with
  `WireProjectRequest::Read(WireProjectReadRequest)`.
- `ResourceSummarySpecifier`: replace with resource read queries using the
  shared `WireReadLevel` vocabulary.
- `RuntimeBufferPayloadSpecifier`: replace with explicit resource detail or
  payload requests.
- `ProjectGetChangesOptions`: rename/rework into read-query options on the
  client side.
- `slot_watch_roots`, `watch_slot_root`, `unwatch_slot_root`, and
  `slot_watch_specifier` in `ProjectView`: replace with stateless read helpers
  or remove from the first client mirror if not needed.

### Delete

- `WireSlotWatchSpecifier` once no callers remain.
- Any `GetChanges`, watch, detail-toggle, or disabled-sync vocabulary that is
  only supporting the removed project sync model.

## Testing Shape

The first plan should prove the protocol at three levels:

- `lpc-wire` serialization tests for read requests, read responses, LOD
  queries, resource payload requests, and probe requests.
- `lpc-engine` or `lpa-server` handler tests showing a loaded basic project can
  answer a full read request without server-side client state.
- `lpc-view` application tests showing a `ProjectView` can apply the full read
  response into node tree, slot mirror, and resource cache.

Preferred visible evidence test:

- load `examples/basic`;
- issue `ReadProject { since: None, queries: default_debug(), probes: [] }`;
- print or assert the returned revision, node count, slot root count, resource
  summary count, and stable ordering;
- apply it to a client view;
- issue a second `ReadProject { since: Some(revision), ... }` and assert it is
  empty or contains only intentional changed summaries.

Probe tests should be narrow in the first slice:

- wire-level round trip for at least one visual probe request/result;
- wire-level round trip for a consumed-slot resolution/explain probe if the
  type is introduced in this plan;
- engine execution only if the current visual render path can support it
  without becoming the main work of this plan.

### Model

- `lpc-model` owns the core portable vocabulary:
  - `Revision`
  - `NodeId`
  - `TreePath`
  - `SlotPath`
  - `SlotShapeRegistrySnapshot`
  - `SlotData`
  - `LpValue`
  - `ResourceRef`
  - `ProductRef`
- `SlotShapeRegistry` supports whole snapshots today.
- Registry diff support may be needed soon but can be deferred if the first
  implementation sends full registry snapshots.

## Suggested Protocol Vocabulary

Naming rule: inside `lpc-wire`, do not prefix every new type with `Wire`.
Use concise names like `ProjectReadRequest`, `ProjectReadQuery`, and
`ExplainSlotProbe`. Keep `Wire` only where it disambiguates from existing
model/engine concepts or from older exported API that already uses the prefix.

### Request

```rust
pub struct ProjectReadRequest {
    pub since: Option<Revision>,
    pub queries: Vec<ProjectReadQuery>,
    pub probes: Vec<ProjectProbeRequest>,
}

pub enum ProjectReadQuery {
    Shapes(ShapeReadQuery),
    Nodes(NodeReadQuery),
    Resources(ResourceReadQuery),
}
```

### Probes

```rust
pub enum ProjectProbeRequest {
    RenderProduct(RenderProductProbeRequest),
    ExplainSlot(ExplainSlotProbeRequest),
    // Future: ShaderPixel(ShaderPixelProbeRequest),
    // Future: ShaderTrace(ShaderTraceProbeRequest),
    // Future: ControlBuffer(ControlBufferProbeRequest),
    // Future: Filesystem(FilesystemProbeRequest),
    // Future: Io(IoProbeRequest),
}
```

### Shared LOD

```rust
pub enum ReadLevel {
    Ids,
    Summary,
    Detail,
}
```

### Response

```rust
pub struct ProjectReadResponse {
    pub revision: Revision,
    pub results: Vec<ProjectReadResult>,
    pub probes: Vec<ProjectProbeResult>,
}

pub enum ProjectReadResult {
    Shapes(ShapeReadResult),
    Nodes(NodeReadResult),
    Resources(ResourceReadResult),
}
```

```rust
pub enum ProjectProbeResult {
    RenderProduct(RenderProductProbeResult),
    ExplainSlot(ExplainSlotProbeResult),
    // Future: ShaderPixel(ShaderPixelProbeResult),
    // Future: ShaderTrace(ShaderTraceProbeResult),
    // Future: ControlBuffer(ControlBufferProbeResult),
    // Future: Filesystem(FilesystemProbeResult),
    // Future: Io(IoProbeResult),
}
```

Suggested semantics:

- `since: None` requests full data for the selected queries.
- `since: Some(rev)` requests changes since `rev` for the selected queries.
- `probes` are evaluated at the response revision, but are not part of the
  persistent client mirror unless a client stores them locally.
- The server never remembers the request after responding.
- The response revision is the authoritative revision at which the response was
  produced.

## Open Questions

### Q1. Does This Work Need A Fresh Roadmap?

Context: The old slot-domain roadmap contains a lot of now-completed M1/M2
work and stale M3 assumptions. The new protocol is clear enough to plan, but it
will likely lead to follow-up plans for view/UI, mutation, and resource payloads.

Suggested answer: do not make a full new roadmap yet. Use this standalone plan
for stateless project read sync. After this lands, decide whether the remaining
work should become a fresh "client interface rebuild" roadmap.

### Q2. Which Domains Are In The First Plan?

Context: User mentioned resource, node, maybe artifact. The decision is now
node-centric UI first: artifacts are internal details, and the filesystem API is
the user-visible path for direct file inspection/editing. The engine can already
answer node/tree, slot shapes, slot data roots, and resource metadata.

Suggested answer: include typed query/result variants for `Shapes`, `Nodes`,
and `Resources`. Do not add an artifact domain in the first read protocol.
Node detail may include internal source/def handles when useful for debugging,
but the client-facing model should stay node-centric. Keep product
materialization out of the normal domain query list and model it as `probes` on
the read request instead. Implement full behavior for shapes, nodes, and
resources first. Implement render-product probing as a late phase if the engine
path is straightforward.

### Node-Centric Exposure Notes

Nodes have several related pieces of data:

- **Def:** authored node data, usually artifact-backed, exposed to clients as
  the node's authored/config slot root.
- **Invocation:** parent-owned placement of a node in the tree. For now expose
  only basic placement/path info unless more is already available.
- **Bindings:** authored dataflow connections attached to nodes. Expose enough
  summary/detail to explain how consumed slots are wired.
- **Resolved consumed slots:** runtime observations of what a node actually
  sees after binding resolution. These are useful debug/detail data, not the
  mutation target.
- **Produced slots:** runtime-owned outputs, including product handles.

The read protocol should expose this from the node outward. It should not make
clients choose between "artifact mode" and "node mode." Direct file editing
remains available through the existing filesystem API.

Resolved consumed values have two UX levels:

- **Last-tick values:** values the node actually resolved during normal
  execution. These belong in node detail because they answer the everyday
  question "what did this node actually use?"
- **Explicit resolve/explain:** client-requested resolution of a consumed slot,
  potentially with provenance/trace. This is a probe because it asks the runtime
  to perform extra diagnostic work for inspection.

Suggested rule: if the node touched it during normal execution, expose it as
node detail. If the client asks the runtime to resolve/explain something just
for inspection, expose it as a probe.

### Q3. How Much Incremental Diffing In The First Slice?

Context: `since: Option<Revision>` should be in the protocol now. However,
slot registry diffs and slot data patch generation across all roots are more
work than initial full snapshots.

Suggested answer: wire the `since` field now, but implement initial full
snapshot behavior first. For `since: Some`, return changed ids/summaries where
already easy, and allow detail results to be full replacements for selected
roots. True minimal slot patches can be a later phase or later plan.

### Q4. What Is The Basic/Local Convenience Query?

Context: Basic/local clients need an easy way to get useful data without a
fancy conversation, but embedded clients need precise low-bandwidth control.

Suggested answer: add a client-side helper/preset, not a separate server mode.
For example `WireProjectReadQuery::default_debug()` or a helper that builds:

- shape detail/full registry
- all node summaries
- node detail for conventional runtime `state` roots
- resource summaries

Do not include resource payload bytes by default.

### Q5. Should The Server Return Result Items In Query Order?

Context: One envelope can contain many domain queries. Deterministic ordering
helps testing and simple clients.

Suggested answer: yes. Results should align with request query order, and each
domain result should use deterministic ordering internally.

### Q6. How Should Product Materialization Work For Client Inspection?

Context: Some projects may never materialize a shader into an intermediate
texture. A fixture or output may sample a `VisualProduct` directly, leaving no
resource buffer that a UI can fetch. Users still need to inspect what a shader
is doing, often by asking the server to render a visual product into an
inspection texture on demand. This is product probing, not resource sync.
`Probe` is the preferred noun: like an oscilloscope probe, it is a diagnostic
tap over an existing signal/product, not a new authored graph edge or stored
resource.

Possible approaches:

- Expose node-specific debug slots such as `state.debug_texture` or
  `debug.texture`.
- Let clients request product materialization directly on the project read
  request as a probe.
- Return materialized probe detail in the response's probe results.

Suggested answer: do not make debug textures into special node state slots for
the canonical path. Product handles are already first-class `LpValue` payloads,
so the cleaner model is a `probes` list on the read request that asks to
materialize selected products with explicit parameters. The response should keep
probe results separate from normal project mirror results, even if the engine
internally uses a scratch buffer to produce the bytes. It should not create or
imply a registry-owned `ResourceRef` unless the product is explicitly
persisted/promoted into a resource by some future feature. A future streaming
transport can avoid storing the full payload, but the protocol should not
require that optimization now. Probe results are diagnostic and path-specific:
a render-product probe can work even if sample rendering is buggy, or vice
versa.

### Q7. Should Consumed Slot Resolution Be A Probe?

Context: A node inspector often wants effective values for slots the node
actually used during normal execution. That is normal node detail. A deeper
debugging workflow wants to ask "resolve this consumed slot now and explain
where it came from." That should bypass the normal resolver cache and re-run
resolution with tracing enabled, because the client is asking for diagnostic
work rather than mirrored project state.

Suggested answer: yes, explicit consumed-slot resolution/explanation should be
a probe. Use the name `ExplainSlot`: it describes the user's goal, while
re-resolution with tracing enabled is an implementation detail. Full engine
execution can be deferred if necessary, but the wire shape should leave room for
returning both the resolved value and a resolution trace.

### Q8. How Do Shader Debug Probes Fit?

Context: A major long-term capability of the in-CPU shader engine is shader
inspection: render a shader, pick a pixel/sample, and get detailed debug or
trace information about how that sample was produced. This is not ordinary
node state and not resource sync. It is explicitly diagnostic and may be
expensive.

Suggested answer: model shader debug as future probes. In the first wire enum,
include commented-out future variants such as `ShaderPixel` and `ShaderTrace`
to make the intended expansion visible when reading the code, but do not
implement them in this plan.
