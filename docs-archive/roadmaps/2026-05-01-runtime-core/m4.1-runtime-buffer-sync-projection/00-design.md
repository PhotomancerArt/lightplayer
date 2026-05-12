# M4.1 design: runtime buffer and detail sync projection

## Scope of work

M4.1 restores client-visible detail/state parity on top of the M4 core runtime
while moving heavy runtime data to explicit store-backed resource sync.

The milestone keeps `GetChanges` as the core sync envelope, adds summary and
payload specifiers for runtime buffers and render products, and updates the
client view to cache resources. Node details carry semantic resource refs, while
payload bytes are sent only when the client explicitly asks for those resources.

Out of scope:

- source reload/deletion/teardown parity, owned by M4.2;
- multi-shader/shared texture behavior parity, owned by M4.3;
- the long-term core node state model, owned by M4.5;
- compression, chunking, previews, and render-product LOD implementation.

## File structure

```text
lp-core/
├── lpc-model/src/
│   ├── resource.rs                         # NEW: shared ResourceRef, ResourceDomain, RuntimeBufferId, RenderProductId
│   └── lib.rs                              # UPDATE: export shared resource identity types
│
├── lpc-engine/src/
│   ├── render_product/
│   │   ├── render_product_id.rs            # MOVE/RE-EXPORT: id comes from lpc-model
│   │   ├── render_product_store.rs         # UPDATE: summary/materialize accessors
│   │   └── texture_product.rs              # UPDATE: raw texture materialization metadata
│   ├── runtime_buffer/
│   │   ├── runtime_buffer_id.rs            # MOVE/RE-EXPORT: id comes from lpc-model
│   │   └── runtime_buffer_store.rs         # UPDATE: iter summaries, no id reuse invariant
│   ├── node/
│   │   ├── contexts.rs                     # UPDATE: add resource init context or adjacent type
│   │   └── node.rs                         # UPDATE: init hook for node-owned resources
│   ├── nodes/core/
│   │   ├── shader_node.rs                  # UPDATE: allocate/render-product ref in init
│   │   ├── fixture_node.rs                 # UPDATE: allocate fixture colors buffer, expose refs
│   │   ├── output_node.rs                  # UPDATE: allocate/output buffer ref in init
│   │   └── texture_node.rs                 # UPDATE: detail/status projection support
│   └── project_runtime/
│       ├── detail_projection.rs            # NEW: core runtime -> legacy/detail compatibility projection
│       ├── resource_projection.rs          # NEW: store summaries and payload projection
│       ├── core_project_runtime.rs         # UPDATE: GetChanges includes details/resources
│       └── project_loader.rs               # UPDATE: stop manually allocating node resources
│
├── lpc-wire/src/
│   ├── project/
│   │   └── api.rs                          # UPDATE: GetChanges request fields/specifiers
│   ├── legacy/project/
│   │   └── api.rs                          # UPDATE: response resource summaries/payloads, compatibility state refs
│   └── legacy/nodes/
│       ├── fixture/state.rs                # UPDATE: compatibility resource wrapper for lamp_colors
│       ├── output/state.rs                 # UPDATE: compatibility resource wrapper for channel_data
│       ├── texture/state.rs                # UPDATE: compatibility render-product/texture ref for texture_data
│       └── shader/state.rs                 # UPDATE: semantic render product ref if needed
│
└── lpc-view/src/
    ├── project/
    │   └── project_view.rs                 # UPDATE: apply details/resources, resource cache, dev auto-watch support
    └── resource_view.rs                    # NEW: client-side resource summary/payload cache
```

## Conceptual architecture

```text
Client GetChanges request
  ├─ since_frame
  ├─ node detail specifier
  ├─ resource summary specifier
  │    └─ domains: buffers / render_products / all
  ├─ buffer payload specifier
  │    └─ none / all / ids
  └─ render payload specifier
       └─ none / all / ids, future options for LOD/preview

Server CoreProjectRuntime::get_changes
  ├─ node summary changes
  ├─ detail_projection
  │    └─ node details with semantic compatibility fields containing refs
  ├─ resource_projection
  │    ├─ store summaries for requested domains
  │    ├─ buffer payloads for requested buffer refs
  │    └─ render product materialized texture payloads for requested render refs
  └─ response

Client ProjectView::apply_changes
  ├─ updates node tree/detail state
  ├─ updates resource summary cache
  ├─ updates resource payload cache
  └─ existing dev UI/helper calls read from semantic state refs + cache
```

## Main components

### Shared resource identity

Move the small resource id newtypes into `lpc-model` so `lpc-engine`,
`lpc-wire`, and `lpc-view` share one identity vocabulary. Stores remain in
`lpc-engine`.

Resource refs use `{ domain, id }`. Store ids are monotonic and never reused
during the lifetime of a loaded project runtime. Removed ids are permanently
invalid for that runtime; recreated resources get new ids.

### GetChanges resource specifiers

`GetChanges` remains the only sync envelope. The client sends all current
interests on every request; the server holds no resource subscription/session
state.

M4.1 adds:

- resource summary specifiers for buffers, render products, or all resources;
- runtime-buffer payload specifiers (`None`, `All`, `ByIds`);
- render-product payload specifiers (`None`, `All`, `ByIds`).

One `since_frame` applies to the response. Clients that want separate cadences
can issue multiple `GetChanges` streams with different frames and specifiers.

### Node details and compatibility state

Node details should expose resource refs in semantic field positions. For M4.1,
that means explicit compatibility wrappers inside the existing legacy
`NodeState` structs rather than a flat `node_resources` list.

Examples:

- `OutputState.channel_data` points at an output-channel buffer.
- `FixtureState.lamp_colors` points at a fixture-colors buffer.
- `TextureState.texture_data` can point at a render product or remain
  metadata-only if texture nodes do not own visual payloads.
- `FixtureState.mapping_cells` remains an inline compatibility snapshot in
  M4.1.

These compatibility additions should be easy to find later, using names/docs
that mention `legacy` or `compatibility`. M4.5 owns the durable core node state
model.

### Resource summaries and payloads

Summaries power list pages, skeleton resource boxes, and client cache pruning.
They include ids, domains, kinds, metadata, changed frames, and size hints. Node
details carry refs only; clients cross-reference summaries when they want
metadata.

Payloads are sent only when requested:

- runtime-buffer payloads send metadata plus raw bytes;
- render-product payloads ask the render product to materialize full/native
  texture bytes with width, height, and format.

LOD, preview, chunking, compression, and regions are future render-product watch
options. The M4.1 wire shape should leave room for them, but only full/native
payloads need implementation.

### Node-owned resource initialization

M4 currently lets `CoreProjectLoader` allocate placeholder resources and pass ids
into node constructors. M4.1 should replace this shortcut with a narrow
node-owned resource init path.

Core nodes allocate owned buffers/products during init or attachment through a
small resource init context. The loader still provides config and orchestration,
but it should not know each node's internal resource needs.

### Dev UI behavior

The current UI is temporary. M4.1 should make it work plainly, not polish it.
It may auto-request store summaries and resource payloads for watched node
details. Manual `just demo` validation remains part of acceptance.
