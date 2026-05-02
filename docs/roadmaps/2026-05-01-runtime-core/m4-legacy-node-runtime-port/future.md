# M4 tactical shortcuts / follow-ups

## Source path to `TreePath` (phase 3 loader)

- Filesystem dirs under `/src` use `name.<kind>` for leaves. Intermediate path
segments without a kind suffix become synthetic `*.folder` segments so every
`TreePath` segment stays `name.ty`.
- Names are sanitized for `NodeName` (`-` → `_`; other unsupported characters
are errors). Collisions are possible after sanitization.
- The node tree indexes siblings by **parent + child name only** (not type).
Duplicate child names under one parent with different types are unsupported.

## Discovery

- `discover_legacy_node_dirs` only lists **immediate** children of `/src`.
Deeper nested authored trees are not loaded until discovery is extended.

## Placeholder nodes

- `CorePlaceholderNode` was useful while phases 1-3 built the loader before real
texture/shader/fixture/output nodes landed. It remains available for tests and
future non-MVP node kinds, but the authored MVP path now attaches concrete core
nodes.

## Core `ShaderNode` (phase 4)

- **Pre-registered render product id**: The node does not create its
`RenderProductId` itself; the engine (or tests) must `insert` a placeholder
`TextureRenderProduct` first and pass that id into `ShaderNode::new`. This
keeps render output keyed under a stable id for fixtures/bindings without
teaching the node about `RenderProductStore` construction order.
- **CPU copy (M4 scaffold):** Each tick, `ShaderNode` copies pixels out of
`LpsTextureBuf` into `TextureRenderProduct(Vec<u8>)` for the store. Acceptable
for the port; consider zero-copy or mapped storage where the product can own
or alias the graphics buffer.
- **Multi-shader / render order**: Legacy runs every shader targeting a texture
in `render_order` into one shared `LpsTextureBuf` owned by a computed “buffer
owner” node. The core path only exercises independent `ShaderNode` ticks; it
does not yet sequence multiple producers into one texture or resolve buffer
ownership ties—add an orchestration pass or shared target product when
porting fixture-driven lazy rendering.

## Core output sink / flush (phase 6)

- **Post-tick flush:** `CoreProjectRuntime::tick` runs `Engine::tick`, then `RuntimeServices::flush_dirty_output_sinks` using an optional boxed `OutputProvider`. No separate engine post-tick hook was added.
- **Sink registry:** GPIO pins, open handles, and `last_byte_count` live in `RuntimeServices` (`register_output_sink`). `OutputNode` is a non-demand leaf with an empty tick for tree/authored placement only; it does not participate in flush or call `OutputProvider::close` on destroy yet — unregister/teardown belongs to later wiring.
- **Dirty rule:** A sink flushes only when its backing `RuntimeBuffer`'s `Versioned::changed_frame` equals the engine frame id after the tick (fixture writes use `TickContext::with_runtime_buffer_mut`). Insert sink buffers with `FrameId::default()` so untouched sinks stay stale across frames until fixtures mutate them.
- **Tests:** `MemoryOutputProvider` is wrapped (`Rc` + a thin `OutputProvider` impl) so the runtime can own `Box<dyn OutputProvider>` while tests keep an `Rc` clone for assertions.

## Fixture core node (phase 5)

- **Direct engine-owned stores**: `Engine` owns `RenderProductStore` and
`RuntimeBufferStore` directly. Node-side access is routed through narrow
`TickContext` services backed by the active `ResolveHost`, so fixtures can
sample render products and mutate a single runtime buffer without
`Rc<RefCell<_>>` store handles. Sampling still goes only through
`RenderProductStore::sample_batch`; no texture byte leaks via
`RuntimePropAccess`.
- **Scoped buffer writes**: Fixture output writes use
`TickContext::with_runtime_buffer_mut` to resize/update the existing sink
buffer in place and bump its frame. This still writes byte payloads directly
from fixture math rather than a typed output-buffer abstraction; revisit when
output sink nodes land.
- **Fixture MVP**: Core `FixtureNode` resolves `QueryKey::NodeOutput { shader, texture output path }`,
batches UV samples for legacy `compute_mapping` entries, reconstructs accumulator math aligned with
`accumulate_from_mapping` (pixels fed from normalized render samples converted to legacy u8), and
writes one u16 triple per channel into a chosen `RuntimeBuffer::raw` sink. Full authored
`FixtureConfig`/transform/compatibility parity is intentionally not wired here yet.

## Runtime output/state access split

- **Now:** Resolver production reads `RuntimeOutputAccess` on `Node` first,
then falls back to scalar `RuntimePropAccess`. `ShaderNode` exposes
`RuntimeProduct::Render` only via `RuntimeOutputAccess`.
- **Later:** Retire `RuntimePropAccess` as a primary abstraction; keep
scalar fallbacks only until spine nodes expose `RuntimeProduct::Value` (or
equivalent) through the output path. `RuntimeStateAccess` is reserved for
sync/debug state snapshots.

## Server/demo cutover compatibility (phase 7)

- `lpa-server::Project` now owns `CoreProjectRuntime` on the active load/tick
path. The server no longer calls legacy runtime load/init/tick for loaded
projects.
- `CoreProjectRuntime::get_changes` currently projects only frame identity,
current core tree handles, created/config/state/status changes, and no
`node_details`. This keeps existing wire clients alive for load/tick requests
while M4.1 owns proper buffer/render-product detail snapshots.
- `CoreProjectRuntime::handle_fs_changes` is a no-op for now so server
filesystem version tracking can advance without keeping the legacy runtime
active. Source reload belongs with the later scene update/runtime sync work.
- Scene update tests now assert this M4 behavior explicitly: modified
`node.toml`, modified GLSL, and node deletion are accepted by the core runtime
hook but do not rebuild, recompile, or remove already loaded core nodes until
source reload lands.
- Unload/stop-all removal currently drops core runtimes without explicit node
destroy/close traversal. Output sink unregister/teardown remains follow-up
work with the output sink lifecycle.

## Server project creation API

- **Idea:** Rework `ProjectManager::create_project` for the core runtime/source
layout or remove it from the server surface if creation remains a CLI-only
concern.
- **Why not now:** M4 cut over loading/ticking existing authored projects; adding
project creation semantics would expand the milestone beyond validation.
- **Useful context:** `lp-app/lpa-server/src/project_manager.rs`,
`lp-cli/src/commands/create/project.rs`

## Compatibility detail sync (phase 8)

- `scene_render` now exercises the authored project through
`CoreProjectLoader`/`CoreProjectRuntime` and still verifies real shader ->
fixture -> output bytes.
- `partial_state_updates` is reduced to the M4 metadata contract:
`GetChanges` reports handles/state metadata, while `node_details` remains
empty. Fixture `lamp_colors`/`mapping_cells` detail deltas move to M4.1's
buffer/render-product-aware sync.
- **Observed demo behavior:** `just demo` starts, loads the example project, and
messages pass through the server/core runtime path, but the client shows nodes as
`(Waiting for state data...)` and no visible data flows because M4 does not yet
project node detail/state payloads or runtime buffer/render-product refs.