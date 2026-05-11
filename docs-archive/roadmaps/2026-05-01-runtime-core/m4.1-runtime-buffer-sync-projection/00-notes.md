# M4.1 notes: runtime buffer and detail sync projection

## Scope of work

M4.1 restores the client-visible detail/state contract on top of the M4 core
runtime while moving heavy runtime data toward explicit store-backed resource
references.

This milestone should:

- make `CoreProjectRuntime::get_changes` return useful `node_details` again;
- get `just demo` past `(Waiting for state data...)`;
- introduce a minimal wire/view model for runtime buffers and render products;
- expose buffer/product metadata, versions, and payload updates needed by the
  current view;
- keep compatibility snapshots only where they are the smallest safe bridge;
- leave source reload, deletion, teardown, and multi-shader behavior to later
  M4.x milestones.

## Current state

### Core runtime projection

`CoreProjectRuntime::get_changes` lives in
`lp-core/lpc-engine/src/project_runtime/core_project_runtime.rs`. As of M4.1
it still projects tree membership, created/config/state/status changes, and
frame ids, and it now fills `node_details` via `detail_projection` / resource
fields per the `GetChanges` request specifiers. See this folder’s `summary.md` for
the delivered scope.

### Existing wire/view detail model

`lp-core/lpc-wire/src/legacy/project/api.rs` defines:

- `ProjectResponse::GetChanges { node_details: BTreeMap<NodeId, NodeDetail> }`
- `NodeDetail { path, config: Box<dyn NodeConfig>, state: NodeState }`
- `NodeState::{Texture, Shader, Output, Fixture}`
- `SerializableProjectResponse`, which serializes concrete config/state variants.

`lp-core/lpc-view/src/project/project_view.rs` expects details for watched
nodes. It stores `NodeEntryView.state: Option<NodeState>` and merges partial
state updates through the legacy `NodeState::merge_from` methods.

The current legacy state types are heavy-byte oriented:

- `TextureState.texture_data: Versioned<Vec<u8>>`
- `OutputState.channel_data: Versioned<Vec<u8>>`
- `FixtureState.lamp_colors: Versioned<Vec<u8>>`
- `FixtureState.mapping_cells: Versioned<Vec<MappingCell>>`

### Runtime buffers and products

`RuntimeBufferStore` owns `Versioned<RuntimeBuffer>` values keyed by
`RuntimeBufferId`. `RuntimeBuffer` already carries domain metadata for texture,
fixture color, output channel, and raw buffers.

`RuntimeProduct` can carry scalar values, `RenderProductId`, or
`RuntimeBufferId`. Render products are intentionally the visual/samplable path;
texture pixels should not be treated as scalar props.

There is no general `ProductDomain`/`DataDomain` enum today. `RuntimeProduct` is
an engine value/product wrapper, not a durable wire taxonomy for store browsing,
resource subscriptions, or payload sync.

### Core node state surfaces

`RuntimeOutputAccess` exists for node-owned outputs, and `RuntimeStateAccess` is
currently marker-only. Core nodes have enough local information to rebuild some
legacy-compatible details, but not through a uniform projection API yet:

- `TextureNode` owns `TextureConfig` and scalar width/height/format props.
- `ShaderNode` owns `ShaderConfig`, GLSL source, compile error, and a render
  product id.
- `OutputNode` owns a runtime buffer id.
- `FixtureNode` owns texture/output/shader ids, mapping config, and the output
  sink buffer id. It computes mapping entries and output bytes but does not
  currently expose fixture runtime state.

### Loader/source metadata

`CoreProjectLoader` clones loaded legacy configs while constructing nodes, but
`CoreProjectRuntime` does not retain a source config/detail index. After load,
`get_changes` can recover tree paths and runtime nodes, but it does not have a
clean typed route to reconstruct `NodeDetail` for every node.

## Confirmation-style questions

| # | Question | Context | Suggested answer |
| --- | --- | --- | --- |
| Q1 | Keep M4.1 focused on sync/detail projection only? | Reload/deletion/teardown are now planned as M4.2. | Yes. |
| Q2 | Preserve existing `ProjectResponse::GetChanges` shape for this milestone? | `lpa-server`, `lpa-client`, and `ProjectView` already use it. | Yes; extend details/resources rather than replacing the response envelope. |
| Q3 | Add a core-runtime detail/config index instead of downcasting core node internals? | Loader has the typed config/source data at construction time; runtime nodes are trait objects. | Yes. |
| Q4 | Treat fixture transform config as out of scope? | M4.3 will remove/quarantine transform config rather than port it. | Yes. |
| Q5 | Leave source reload and resource removal events mostly structural until M4.2? | M4.1 can define removal semantics, but M4.2 creates actual deletion/reload behavior. | Yes. |

Resolved: user accepted the parity-before-M5 direction and confirmed that M4.1
should own the data sync semantics for store-backed resources.

## Discussion-style questions

### Q6: Do resource refs extend legacy `NodeState`, or live beside `node_details`?

The current client wants `NodeState` to stop waiting. We can either add
resource-ref fields to the existing legacy state variants, or add a parallel
resource section to `ProjectResponse`/`SerializableProjectResponse` and keep
legacy state payloads as compatibility snapshots.

**Suggested answer:** Add a parallel runtime resource section to the response
and keep `NodeState` as the compatibility/view state bridge. For M4.1, populate
minimal `NodeState` details so the current client works, and add buffer/product
refs for the heavy fields that should not remain authoritative snapshots.

### Q7: Which fields should be buffer-backed in M4.1 versus compatibility snapshots?

The heavy candidates are texture pixels, output channel bytes, fixture lamp
colors, and possibly fixture mapping cells. Mapping cells are structured
geometry, not raw frame data.

**Suggested answer:** Buffer-back output channel bytes and fixture lamp colors
first. Use render-product refs for shader/texture visual output. Keep mapping
cells as a compatibility detail for M4.1 unless planning proves a typed
structured-buffer path is worth adding now.

### Q8: How should core nodes expose sync/debug state?

`RuntimeStateAccess` is marker-only. We can make nodes expose typed sync state,
or keep projection in `CoreProjectRuntime` using a sidecar index plus engine
stores.

**Suggested answer:** Add a projection-side API owned by `project_runtime`
first. Do not make every `Node` implement a broad sync state surface until the
resource ref model settles. Use sidecar metadata and stores to build details.

### Q9: How much compatibility should initial M4.1 preserve?

The quickest way to unstick the UI is to recreate old `NodeState` snapshots.
The better long-term shape is resource refs plus client caches.

**Suggested answer:** Preserve compatibility at the `ProjectView` boundary, but
make the server response carry resource metadata/payload updates explicitly.
Then update `ProjectView` to hydrate old getters from cache where needed.

### Q10: What are the semantics for sending buffer/render-product payload updates?

Once heavy data leaves embedded `NodeState`, the sync layer needs an explicit
answer for when a client receives buffer or texture payload updates. Inferring
payload delivery implicitly from node details would make resource sync hard to
reason about and difficult to cache.

**Resolved direction:** Use a three-tier LOD model controlled by the client.
Summary sync always carries high-level node data such as status. Detail sync
carries value-domain data plus metadata/ref summaries for other domains.
Payloads for buffers, textures, and render products are only sent when the
client explicitly asks to sync those resource ids. Later this request can grow
LOD, compression, sampling, or region/chunk controls.

This maps to the UX: the node list shows summary/status; clicking a node shows
details with skeleton/resource boxes; clicking a texture/buffer skeleton opts
that resource into payload sync. It also leaves room for dedicated texture and
buffer browsers where resources are explored directly instead of only through
node details.

Resolved: this is the target M4.1 sync model.

## Notes

- The desktop demo startup issue caused by project folder names was fixed in
  M4; M4.1 starts from a runtime that loads the example project.
- Current M4 scene tests intentionally assert metadata-only behavior. M4.1
  should update those tests to assert real detail/resource sync again.
- Data sync semantics are in scope for M4.1: resource refs, client cache
  identity, and client-requested buffer/render-product payload updates are part
  of the milestone, while compression/chunking can stay out of scope.
- Resource payload sync should not be inferred from node detail sync. Node
  details expose refs and metadata; clients choose which resource ids receive
  payload updates to preserve bandwidth on USB serial, Bluetooth, and embedded
  Wi-Fi links.
- Future resource explorer work should add back-references/ownership metadata so
  the UI can show which nodes hold or produce each buffer/render product.

### Q11: Should M4.1 add wire support for store/resource summaries and payload subscriptions?

The three-tier LOD model needs wire types that can represent resource identity,
domain, metadata, summary versions, and explicitly requested payload updates. We
do not currently have a `ProductDomain`/`DataDomain` enum; only engine-local
`RuntimeProduct`, `RuntimeBufferKind`, `RuntimeBufferId`, and `RenderProductId`.

**Suggested answer:** Yes. M4.1 should add simple first-class wire support now:
a resource domain enum, stable resource refs, lightweight resource summaries,
and client-requested payload sync by resource id. Keep buffer and render-product
watch language distinct: buffers are byte/data resources; render products are
logical visual resources that can later support render-specific watch options.
Keep it minimal for M4.1: no compression, chunking, regions, thumbnails, or
back-reference ownership graph yet.

Resolved: add simple wire support now; do not wait until after M4.1.

### Q12: Should clients be able to subscribe to store summary-level data?

Dedicated texture/buffer pages need to list resources without pulling payloads.
Node details also need to show skeleton resource boxes with enough metadata to
be useful.

**Suggested answer:** Yes. Add summary-level resource requests/subscriptions
for resource domains (`buffers`, `render_products`, or all). Summaries should be
small: id/ref, domain, kind, metadata, changed frame/version, size hints, and
availability/status. Payload sync remains separately requested per resource id.

Resolved: summary-level store/resource sync is in scope to power list pages and
resource skeletons without pulling payloads.

### Q13: How should render products appear in the wire protocol?

Render products exist so the engine can avoid full texture rendering when only
sampling or lazy visual output is needed. From the user's point of view there is
still a texture-like visual resource, but implementation-wise it may be lazy,
sampled, GPU-backed, or CPU-backed.

**Resolved direction:** Treat render products as their own wire resource domain,
not as runtime buffers. Node details can reference a render product summary.
Debug/detail payload sync for render products should be requested through a
render-product watch, which may ask the product to materialize raw texture data
for inspection. The M4.1 payload can be simple raw texture output, but the wire
shape should leave room for render-specific options such as requested LOD,
resolution, sampling mode, or preview thumbnails.

Resolved: render-product watches are distinct from runtime-buffer watches.

### Q14: Should resource watches live inside `GetChanges`, or become separate project requests?

Current wire has one project request path:
`WireProjectRequest::GetChanges { since_frame, detail_specifier }`. We could
extend that request with resource watch specs, or add new project requests like
`ListRuntimeResources` / `GetRuntimeResourcePayloads`.

**Resolved direction:** Extend `GetChanges`. It is the core sync message and
the main client/server interaction path for now. All M4.1 summary/detail/resource
sync should be represented as additional request/response fields under
`GetChanges`, not separate one-off project requests. This can be revisited later
if the protocol grows beyond what one sync envelope can express.

### Q15: What identity should wire resource refs use?

One option was `{ domain, id, generation }` to guard against id reuse. The
simpler invariant is to never reuse resource ids within a loaded project runtime
session.

**Resolved direction:** Use `{ domain, id }` for wire resource refs. Store ids
are monotonically allocated and never reused during the lifetime of a loaded
project runtime. Removal invalidates that id permanently; reload/recreate gets a
new id. If clients want to prune stale local resources, they should subscribe to
the summary view of the relevant stores and compare their cache against the
current summary set.

### Q16: What should M4.1 send for actual payloads?

For buffer watches, payload is straightforward: metadata plus bytes when
changed. For render-product watches, the wire language stays render-specific,
but M4.1 still needs a first concrete payload shape.

**Resolved direction:** Use full/native payloads for now. Buffer payload watches
return metadata plus raw bytes. Render-product watches ask the product to
materialize its current native texture and return width, height, format, and raw
bytes. Preview/LOD/compression/chunking can be represented later, but M4.1
implements full payload sync only.

### Q17: Where should resource id newtypes live?

`RuntimeBufferId` and `RenderProductId` currently live in `lpc-engine`. Once
M4.1 exposes them through sync, they are no longer purely engine-internal. One
option was to duplicate them as `WireRuntimeBufferId` / `WireRenderProductId`.

**Resolved direction:** Move the shared id newtypes into `lpc-model` and reuse
them from `lpc-engine`, `lpc-wire`, and `lpc-view`. Do not maintain parallel
engine and wire id types unless a later protocol boundary proves it necessary.
Keep store implementations in `lpc-engine`; only the small copyable id/ref
types move to the shared model layer.

### Q18: Are resource watches persistent client subscriptions or per-request specifiers?

`GetChanges` can either mutate server-held client session state ("watch this
resource until unwatched") or carry the complete current watch set on each
request.

**Resolved direction:** Use per-request specifiers. The client sends all current
detail handles, store summary domains, buffer payload ids, and render-product
payload ids on every `GetChanges`. The server does not hold subscription/session
state, so it does not need timeout, disconnect, or cleanup logic. This matches
the existing sync style and keeps embedded/server behavior simpler.

### Q19: What does `since_frame` mean for resource payloads?

`GetChanges` currently has one `since_frame` for node changes. Resource watches
could use that same frame or add per-resource known versions.

**Resolved direction:** Use one `since_frame` for M4.1. Watched resource
summaries/payloads are included when their changed frame is newer than the
request frame, or when the client intentionally asks from `FrameId::default()`.
Clients that want different cadences can issue multiple `GetChanges` requests
with different watch sets and frames, e.g. one stream for node summaries/details
and another slower or user-driven stream for resource payloads.

### Q20: How should store summary and resource detail specifiers work?

Store list pages need summary membership so clients can prune stale resources.
Resource inspectors need payload/detail data for selected ids. Local cases like
`just demo` may have no meaningful bandwidth concern and should be able to ask
for everything.

**Resolved direction:** Use two separate axes in `GetChanges`: a flag/specifier
for sending store summaries, and a separate detail/payload specifier for which
resource ids to sync. The detail specifier should support `None`, `All`, and
`ByIds`. Summary sync can send all summaries for requested domains so clients can
power list pages and prune caches. Payload sync remains opt-in by explicit detail
specifier, with `All` available for local/no-bandwidth-concern scenarios.

### Q21: How should node details reference resources?

Node details could include resource refs only, or refs plus inline summaries. The
inline summary path makes skeletons more self-contained but forces the server to
cross-reference store metadata while projecting node details.

**Resolved direction:** Node details should send the resource ids/refs that the
runtime node carries, and not inline store summaries. The server should keep node
detail projection simple. Clients that need dimensions, byte sizes, formats, or
other metadata should also request store summaries and cross-reference locally.
Most clients will subscribe to summaries anyway, and skeleton UI sizing can be
driven by UX constraints rather than exact resource payload metadata.

### Q22: Should resource refs live beside node state or inside semantic state fields?

Resource refs could be projected as a flat `node_resources` list, but that loses
the semantic field position unless we add another keying scheme. The natural
place for a fixture lamp-colors buffer ref is the logical `lamp_colors` field;
the natural place for an output buffer ref is the output channel-data field.

**Resolved near-term direction:** Keep resource refs in semantically named node
detail/state fields for M4.1. Do not introduce a flat `node_resources` list
unless the field-keying design is made explicit. This may mean extending the
compatibility state structs with ref-bearing field variants or adding parallel
core detail structs that preserve semantic field names.

**Future note:** The long-term node-state strategy needs a separate design pass.
M4.5 tracks this. We need to decide whether node state remains a compatibility
concept, becomes a typed core detail model, or is replaced by value/resource
field projections. Any M4.1 compatibility state additions should be named or
organized so they are easy to find later (for example with `legacy` or
`compatibility` in the type/module/docs).

### Q23: For M4.1, should compatibility state refs extend legacy wire structs?

M4.1 needs a practical bridge before M4.5 designs the durable core state model.
The current `NodeState` variants are already consumed by `ProjectView`, so the
shortest path is to keep semantic fields in those variants while making
resource-backed fields explicit and searchable.

**Resolved direction:** Use explicit compatibility field wrappers inside the
existing legacy state structs for M4.1. Semantic fields like fixture
`lamp_colors` and output `channel_data` stay where users expect them, but their
types can indicate inline compatibility data versus a resource ref. Name the
wrappers/modules with `legacy` or `compatibility` so M4.5 can find and replace
them.

### Q24: Which fields should use compatibility resource wrappers in M4.1?

Heavy byte/visual fields are the target. Scalar/value fields can stay as ordinary
compatibility state.

**Resolved direction:** Wrap only payload-heavy fields in M4.1:

- `TextureState.texture_data`: render product ref or omitted if texture nodes are
  metadata-only.
- `OutputState.channel_data`: runtime buffer ref.
- `FixtureState.lamp_colors`: runtime buffer ref if fixture colors become a
  first-class buffer in this milestone; otherwise a compatibility snapshot.
- `FixtureState.mapping_cells`: inline compatibility snapshot for M4.1.
- `ShaderState`: expose the shader render product through the semantic
  output/detail path, not by overloading `glsl_code`.

### Q25: Should M4.1 create a fixture colors runtime buffer?

`FixtureNode` currently computes fixture color accumulators and writes directly
to the output sink buffer. Legacy fixture state had `lamp_colors`, which is
important current UI functionality for visualizing fixtures.

**Resolved direction:** Add a first-class fixture colors runtime buffer in M4.1.
Fixture nodes should write visualization/lamp RGB bytes to a
`RuntimeBufferKind::FixtureColors` buffer and output-formatted bytes to the
output sink buffer. `FixtureState.lamp_colors` should point at the fixture color
buffer through the M4.1 compatibility resource wrapper rather than returning to
large inline state as the source of truth.

### Q26: Who should allocate resource ids for node-owned buffers/products?

M4 currently uses a shortcut: `CoreProjectLoader` inserts placeholder resources
into engine stores and passes ids into node constructors. For example,
`ShaderNode` receives a pre-existing render product id, and output/fixture
paths receive pre-existing runtime buffer ids. This made M4 easy to wire but
feels odd as a durable node API.

Open question: should M4.1 keep loader-side allocation for now, or add a node
resource allocation API so nodes ask a context/store for resources they own?

**Resolved direction:** Prefer node-owned resource creation during
init/attachment. The loader should provide config and orchestration, not know
which buffers/products every node needs. Add a small resource init context if
feasible in M4.1 so nodes can allocate owned buffers/products before first sync
details are projected. If this becomes too large, keep the M4 shortcut briefly
but make it an explicit early M4.x cleanup item.

### Q27: Should M4.1 implement node-owned resource allocation as an early phase?

Node-owned resource creation is a cleanup prerequisite for a clean sync
projection: details should ask nodes for semantic resource refs rather than
reconstructing loader side tables.

**Resolved direction:** Include node-owned resource init as an early M4.1 phase.
It is acceptable for M4.1 to have several phases. Add a narrow
`NodeResourceInitContext`/equivalent, let core nodes allocate owned render
products/runtime buffers during init or attachment, and then build the sync
projection on that cleaner ownership model.

### Q28: What is the minimum demo parity target for M4.1?

M4.1 could stop when node details populate, or it could exercise actual resource
payload sync end to end.

**Resolved direction:** The data path should be real: node details, store
summaries, buffer payload watches, render-product full texture payload watches,
and `ProjectView` resource caching should work in tests. The current UI is a
temporary dev UI, so it only needs to work plainly. It may auto-subscribe to
resources for selected details to keep the UX simple while the protocol still
supports explicit per-request watches.

### Q29: Should M4.1 update `just demo` behavior directly?

The current UI is temporary and not easy for agents to validate visually, but it
is still the user's manual validation path.

**Resolved direction:** Include a minimal dev demo/client path update. The dev UI
can auto-request resource summaries and payloads for watched node details, and
existing helpers should read from the resource cache where appropriate. Do not
build fancy UI panes or an agent-capable CLI inspector in M4.1. Manual user
validation of `just demo` remains part of the acceptance path.
