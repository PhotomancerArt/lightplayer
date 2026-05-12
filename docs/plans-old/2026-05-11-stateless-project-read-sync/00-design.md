# Stateless Project Read Sync Design

## Scope Of Work

Build the first canonical project read protocol after the legacy project-sync
demolition.

The protocol is:

- node-centric;
- stateless with respect to clients;
- pull-only;
- revision-based;
- one project read operation with normal data queries plus request-scoped
  probes;
- explicit about resource payloads and expensive diagnostics.

Artifacts are not a first-class UI/query domain in this plan. They remain an
internal authored-storage detail. Users who want to see or edit files use the
filesystem API.

## Naming

Inside `lpc-wire`, new types should not be prefixed with `Wire` unless the
prefix disambiguates them from existing model/engine concepts or older exported
API. Prefer concise names such as `ProjectReadRequest`, `ReadLevel`,
`NodeReadQuery`, and `ExplainSlotProbeRequest`.

Existing `Wire*` types can remain when they are already established and useful,
for example `WireProjectHandle`, `WireTreeDelta`, and `WireSlotFullSync`.

## File Structure

```text
lp-core/lpc-wire/src/project/
  mod.rs
  wire_project_handle.rs
  wire_project_request.rs
  read/
    mod.rs
    project_read_request.rs
    project_read_response.rs
    read_level.rs
    shape_read.rs
    node_read.rs
    resource_read.rs
    probe/
      mod.rs
      project_probe.rs
      render_product_probe.rs
      explain_slot_probe.rs

lp-core/lpc-engine/src/engine/
  project_read.rs
  project_read_nodes.rs
  project_read_resources.rs
  project_read_probes.rs

lp-core/lpc-view/src/project/
  project_view.rs
  apply_project_read.rs
  resource_cache.rs

lp-app/lpa-client/src/
  client.rs

lp-app/lpa-server/src/
  handlers.rs

docs/lp-core/
  probes.md
  overview.md
```

## Architecture Summary

`ClientRequest::ProjectRequest { handle, request }` remains the outer
project-scoped request envelope. `WireProjectRequest::SyncDisabled` is replaced
with `WireProjectRequest::Read(ProjectReadRequest)`.

`ProjectReadRequest` contains:

- `since: Option<Revision>` for full snapshot versus changes since a known
  revision;
- `queries: Vec<ProjectReadQuery>` for normal mirrorable project data;
- `probes: Vec<ProjectProbeRequest>` for request-scoped diagnostics.

`ProjectReadResponse` contains:

- `revision: Revision`, the authoritative revision for the response;
- `results: Vec<ProjectReadResult>`, aligned with query order;
- `probes: Vec<ProjectProbeResult>`, aligned with probe request order.

The server stores no client subscription state. A client can poll by sending
the latest revision it has. The server answers from current engine state and
then forgets the request.

## Normal Read Domains

### Shapes

Shape reads expose the slot shape registry needed to interpret slot roots.

The first implementation may send a full registry snapshot for detail reads.
Registry diffs can come later.

### Nodes

Nodes are the main project UI model. Node detail can include:

- id/path/status/children;
- authored def/config slot root;
- runtime state slot root;
- produced slot values exposed through runtime state;
- binding summaries/details;
- last-tick resolved consumed values if available.

Artifacts are not exposed as their own query domain. Source/def handles may
appear in node detail as implementation metadata when useful for debugging, but
the client-facing concept is still the node.

### Resources

Resource reads expose store-backed resources such as runtime buffers. Summaries
and payload bytes are requested explicitly. Payloads are not included in the
default debug request.

Existing resource summary/payload structs can be reused initially, wrapped in
the new read query/result vocabulary.

## Probes

Probes are request-scoped diagnostics. They are not subscriptions, not authored
graph state, and not persistent resources. A probe is allowed to do extra work,
and clients must request it explicitly.

Initial probe vocabulary:

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

`RenderProduct` asks the engine to make a product render into inspection bytes.
It remains product/probe data, not a resource sync result.

`ExplainSlot` asks the engine to explain a slot. For consumed slots this should
eventually bypass the normal resolver cache and re-resolve with tracing
enabled. The first implementation can add the wire vocabulary before fully
executing the engine path.

Future shader probes should support a powerful debugging workflow: render or
sample a shader, pick a pixel/sample, and return detailed CPU-engine debug
information about how that result was produced.

## Client View

`ProjectView` should become a node-centric mirror shell that owns:

- current revision;
- node tree view;
- slot mirror;
- resource cache.

Old watch-style state such as `slot_watch_roots` should be removed once the new
read/apply path exists.

Applying a full read response should update the existing `NodeTreeView`,
`SlotMirrorView`, and `ClientResourceCache` rather than inventing parallel
mirror state.

## Testing Strategy

Wire tests:

- request/response JSON round trips;
- read levels;
- node/resource/shape query variants;
- probe request/result variants;
- commented future variants remain comments only.

Engine/server tests:

- load `examples/basic`;
- answer `ReadProject { since: None, queries: default_debug(), probes: [] }`;
- assert deterministic ordering and useful counts;
- answer a second read with `since: Some(revision)`.

View tests:

- apply a full read response;
- verify tree, slot mirror, resources, and revision update as expected.

End-to-end smoke:

- `lpa-client` sends `ProjectReadRequest`;
- `lpa-server` handler returns a `ProjectReadResponse` instead of the
  sync-disabled error.
