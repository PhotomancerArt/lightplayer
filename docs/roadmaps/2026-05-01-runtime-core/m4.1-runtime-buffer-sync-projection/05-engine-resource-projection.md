# Phase 5: Engine resource projection

## Scope of phase

Implement server-side projection from `CoreProjectRuntime` into node details,
resource summaries, and explicit resource payload updates.

In scope:

- Add `project_runtime/detail_projection.rs`.
- Add `project_runtime/resource_projection.rs`.
- Populate `GetChanges` node details again.
- Populate requested store summaries.
- Populate requested runtime-buffer and render-product full/native payloads.

Out of scope:

- Client-side cache/application logic.
- Source reload/deletion/teardown.
- Render-product LOD/preview/compression.

## Code organization reminders

- Keep projection logic separate from core engine scheduling.
- Entry points first, helpers near the bottom.
- Keep compatibility projection clearly marked as compatibility.
- Do not make `RuntimePropAccess` carry render products.

## Sub-agent reminders

- Do not commit.
- Stay within projection.
- Do not suppress warnings or weaken tests.
- If missing node metadata prevents detail projection, stop and report the
  smallest missing index/API.

## Implementation details

Read:

- `00-design.md`.
- `lp-core/lpc-engine/src/project_runtime/core_project_runtime.rs`
- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/runtime_buffer/runtime_buffer_store.rs`
- `lp-core/lpc-engine/src/render_product/render_product_store.rs`
- `lp-core/lpc-engine/src/nodes/core/*`

Implement `detail_projection`:

- Build `NodeDetail` for requested handles.
- Use semantic compatibility fields with resource refs for heavy fields.
- Include config snapshots from a core runtime sidecar/detail index if needed.
- Preserve `NodeChange` summary behavior.

Implement `resource_projection`:

- Iterate runtime buffers and render products for summary requests.
- Include current id sets for requested domains so clients can prune caches.
- Return buffer payloads only for requested ids and changed frames.
- Return render-product full/native texture payloads only for requested ids and
  changed frames.

Render products may need a trait/store method to materialize raw texture data.
Keep it narrow and implemented by `TextureRenderProduct` for M4.1.

Add tests in `lpc-engine` for:

- initial `GetChanges` includes node details for watched nodes;
- summary request returns current buffer/render product ids;
- output buffer payload watch returns bytes;
- render product payload watch returns texture bytes;
- fixture `lamp_colors` detail points at the fixture colors buffer.

## Validate

Run:

```bash
cargo test -p lpc-engine project_runtime
cargo test -p lpc-engine resource
cargo test -p lpc-engine --test scene_render
```
