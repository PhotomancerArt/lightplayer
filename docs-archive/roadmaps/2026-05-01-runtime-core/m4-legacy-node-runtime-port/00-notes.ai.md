# Scope of Work

M4 is the clean-break cutover of the current MVP demo runtime path onto the core
engine. It reworks the useful legacy texture/shader/fixture/output nodes into
first-class `Node` implementations, wires authored projects into a core
`Engine`, and makes `just demo` use the new runtime stack.

The branch may be broken while this happens. The old `LegacyProjectRuntime`
remains in git and may be consulted as reference code, but M4 should not build a
temporary adapter layer around old runtime APIs.

In scope:

- Build source project -> core `Engine` loading for the current authored legacy
  project layout.
- Port texture, shader, fixture, and output behavior into durable core node
  implementations.
- Preserve embedded shader compilation and execution; do not gate the compiler
  behind `std`.
- Use demand-root execution for fixture/output-driven frames.
- Use `RuntimeBufferStore` / `RuntimeProduct::Buffer` for texture pixels,
  fixture colors, and output channel bytes where those are runtime products.
- Reproduce the current shader -> texture -> fixture -> output demo behavior
  closely enough that `just demo` can run on the new stack.
- Rewire `lpa-server` / demo project ticking to the new runtime for the MVP
  project.
- Temporarily project compatibility state for existing wire/view flows where
  needed; proper buffer-aware sync is M4.1.
- Update tests and server/demo wiring as needed for the new path.

Out of scope:

- Building a temporary legacy adapter harness; M3.3 is intentionally
  superseded.
- Full retirement/removal of `LegacyProjectRuntime`; M5 owns cleanup after the
  new demo path proves itself.
- Proper runtime buffer sync refs/diffs/client cache; M4.1 owns that.
- A final render-product family beyond what is required by the MVP path.
- Async/parallel scheduling.
- Disabling embedded shader compilation to make host work easier.

# Current State

## Core Engine Spine

The core engine lives under `lp-core/lpc-engine/src/engine/` and owns:

- `NodeTree<Box<dyn Node>>`;
- `BindingRegistry`;
- `Resolver` and same-frame cache;
- `RenderProductStore`;
- `RuntimeBufferStore`;
- demand roots.

The core `Node` API is in `lp-core/lpc-engine/src/node/`:

- `Node::tick(&mut TickContext)`;
- `Node::destroy(DestroyCtx)`;
- `Node::handle_memory_pressure`;
- `Node::props() -> &dyn RuntimePropAccess`.

Today, `TickContext` can resolve `QueryKey`s through a session and can inspect
artifact frames. It cannot directly mutate runtime buffers, render products,
graphics resources, or output providers. M4 needs a narrow way for ported nodes
to access the resources they own.

`RuntimeProduct` currently supports:

- `Value(LpsValueF32)` for scalar/shader-compatible values;
- `Render(RenderProductId)` for sampleable products;
- `Buffer(RuntimeBufferId)` for engine-owned byte-heavy products.

`RuntimeProduct::Value` rejects `LpsValueF32::Texture2D`. Texture-like products
must use handles.

## Legacy Runtime Path

The production server/demo path still uses `LegacyProjectRuntime`:

- `lp-core/lpc-engine/src/legacy_project/project_runtime/core.rs`
- `lp-core/lpc-engine/src/legacy/project.rs`
- `lp-app/lpa-server/src/project.rs`
- `lp-app/lpa-server/src/project_manager.rs`
- `lp-app/lpa-server/src/server.rs`

Legacy initialization order is texture -> shader -> fixture -> output.

Legacy ticking is fixture-driven:

1. Advance frame/time.
2. Render all fixture nodes.
3. Fixture texture reads lazily trigger shader rendering for the texture.
4. Shaders targeting a texture run by `render_order`; the winning shader owns
   the shared output buffer for that texture.
5. Fixture sampling writes output channel buffers.
6. Dirty outputs flush through `OutputProvider`.

## Legacy Node Behavior To Rework

Texture:

- `lp-core/lpc-engine/src/legacy/nodes/texture/runtime.rs`
- Currently metadata-only for width/height/format.
- `texture_data` wire state is compatibility/snapshot data, not authoritative
  runtime ownership.

Shader:

- `lp-core/lpc-engine/src/legacy/nodes/shader/runtime.rs`
- Uses `Arc<dyn LpGraphics>`.
- Loads GLSL source and calls `graphics.compile_shader`.
- Executes `LpShader::render(&mut LpsTextureBuf, time_seconds)`.
- Output buffer allocation currently goes through `LpGraphics::alloc_output_buffer`.
- Multiple shaders may target one texture; `render_order` and node ID determine
  owner/ordering.

Fixture:

- `lp-core/lpc-engine/src/legacy/nodes/fixture/runtime.rs`
- Reuses mapping/accumulation code under
  `lp-core/lpc-engine/src/legacy/nodes/fixture/mapping/`.
- Samples RGBA16 texture data and writes u16 RGB output channels.

Output:

- `lp-core/lpc-engine/src/legacy/nodes/output/runtime.rs`
- Opens/writes/closes via `lpc_shared::output::OutputProvider`.
- Flushes only when touched for the current frame in the legacy path.

## Source Loading

Current authored test/demo projects still use:

- `/project.json`;
- `/src/<id>.<kind>/node.toml`;
- shader source files such as `main.glsl`.

Relevant loading/building pieces:

- `lp-core/lpc-source/src/legacy/node_loader.rs`;
- `lp-core/lpc-engine/src/legacy_project/legacy_loader.rs`;
- `lp-core/lpc-shared/src/project/builder.rs`.

`SrcNodeConfig` exists in `lpc-source`, but there is no production source
project -> core `Engine` loader yet.

## Tests And Validation Surfaces

Useful current tests:

- `lp-core/lpc-engine/tests/scene_render.rs` for legacy end-to-end render
  behavior.
- `lp-core/lpc-engine/tests/scene_update.rs` for filesystem/source updates.
- `lp-core/lpc-engine/tests/partial_state_updates.rs` for wire state deltas.
- `lp-core/lpc-engine/tests/runtime_spine.rs` for current core engine spine.
- firmware emulator tests for real shader compilation/execution paths.

M4 should expect tests to move/break temporarily while the new runtime path
becomes the primary path.

# Questions

## Confirmation-style Questions

| # | Question | Context | Suggested answer |
|---|----------|---------|------------------|
| Q1 | Is M4 allowed to break demo/tests temporarily during the clean break? | User explicitly said “I’m not scared of a little brokenness.” | Yes. Keep commits honest, but do not over-preserve legacy compatibility mid-port. |
| Q2 | Should M4 keep the current authored project layout initially? | `ProjectBuilder`, demos, tests, and server loading all use `/project.json` + `/src/*.kind/node.toml`. | Yes. Build the new `Engine` from the current layout first; source layout cleanup can follow. |
| Q3 | Should M4 use old runtime code as reference, not as wrappers? | M3.3 was superseded to avoid temporary adapters. | Yes. Move/reuse internals where appropriate, but make public runtime behavior first-class core nodes. |
| Q4 | Should proper buffer-aware sync remain M4.1? | M4.1 now exists for runtime buffer refs/cache/diffs. | Yes. M4 may keep compatibility snapshots to get demo working. |
| Q5 | Should `LegacyProjectRuntime` removal wait for M5? | M4 needs focus on new path, M5 owns cleanup/cutover hardening. | Yes. Stop using it on the demo path if practical, but do not spend M4 deleting every old API. |

## Discussion-style Questions

### Q6: What is the M4 runtime owner type?

The server currently owns `LegacyProjectRuntime` inside `lpa-server::Project`.
The new core `Engine` is lower-level and lacks project loading, output provider,
graphics, and compatibility sync ownership.

Suggested answer: introduce a new `CoreProjectRuntime` (or `ProjectRuntime`) in
`lpc-engine` that owns `Engine` plus project/runtime services: filesystem root,
graphics, output provider, compatibility state projection, and source reload
hooks. `lpa-server::Project` should move toward owning this runtime.

### Q7: How should ported nodes access buffers, graphics, and output provider?

`TickContext` currently exposes resolving and artifact checks, not mutable
engine stores or external services. Ported shader/fixture/output nodes need
runtime buffers, `LpGraphics`, frame time, and output handles.

Suggested answer: extend the core tick path with a narrow runtime services
surface rather than giving nodes the whole `Engine`. For example,
`TickContext` can expose `runtime_buffers`, frame time, and registered services
through focused methods, while project runtime owns graphics/output provider.

### Q8: Who owns shader texture memory?

Legacy shader execution writes into `LpsTextureBuf` allocated by `LpGraphics`.
M3.2 `RuntimeBuffer` owns generic bytes, but shader output textures are visual
products, not generic raw buffers.

Suggested answer: shader/pattern output should be a render product. The public
core path should pass an opaque samplable product handle, backed initially by the
current CPU/JIT texture allocation. `LpsTextureBuf` should stay an implementation
detail behind `LpGraphics` / render-product internals. Full raw-byte
materialization is still a valid operation: on GPU-backed hosts we can copy the
texture to RAM when needed, and on embedded the raw bytes are naturally
available. Runtime buffers remain for non-visual byte products such as fixture
colors, output channels, raw protocol payloads, and compatibility snapshots.

### Q9: How should dependency/scheduling semantics work?

Legacy rendering is lazy texture driven: fixture sampling asks for texture data,
which triggers the target shaders in render order. The core engine is
demand-root + resolver/cache driven.

Suggested answer: fixtures are the demand roots. A fixture resolves the
shader/pattern render products it needs, samples them, and pushes channel/color
data into output sinks. Outputs are special data sinks that fixtures push to;
outputs do not know where their data comes from because fixtures own mapping
information. This should leave room for future many-to-many fixture -> output
mapping. Preserve same-frame caching and multi-shader render-order semantics in
the producer/product path.

### Q10: How aggressive should server/demo rewiring be inside M4?

`just demo` on the new stack means M4 must include demo/server cutover work.
The risk is pulling full cleanup or buffer-sync work too early.

Suggested answer: rewire `lpa-server::Project` to the new runtime in M4 once the
core runtime can load and tick the MVP project. Keep compatibility wire responses
as snapshots until M4.1. Avoid a long-lived dual-runtime switch unless
temporarily needed for debugging. M5 becomes legacy runtime removal and
hardening, not the first runtime cutover.

### Q11: How much parity with old output bytes is required before M4 is done?

A clean break means the old runtime may not be kept live as an oracle. But the
MVP demo should still behave recognizably, and shader/fixture/output behavior is
user-visible.

Suggested answer: require focused parity for the current scene render fixtures
and manual `just demo`/device validation. Do not build a general old-vs-new
harness, but use old tests/reference outputs where they are cheap and useful.

# Answer Log

- Q1-Q5 accepted as suggested.
- Q6 accepted as suggested: introduce a new `CoreProjectRuntime` or
  `ProjectRuntime` in `lpc-engine` that owns `Engine` plus project/runtime
  services such as filesystem root, graphics, output provider, compatibility
  state projection, and source reload hooks. `lpa-server::Project` should move
  toward owning this runtime.
- Q7 accepted as suggested, with emphasis: use the old engine for inspiration,
  but design a good new API for core nodes rather than mirroring legacy
  `RenderContext` shapes.
- Q8 accepted with revision: shader texture memory belongs behind render
  product ownership, not `RuntimeBufferStore`. Render products are opaque and
  samplable; they may materialize raw bytes when needed, but `LpsTextureBuf`
  should not leak through the core node/product API.
- Q9 accepted with revision: fixtures are demand roots. Outputs are pushed data
  sinks, not pull-based demand roots, because fixtures own mapping information.
  Keep room for future many-to-many fixture -> output mapping.
- Q10 accepted with revision: M4 owns the demo/server cutover required for
  `just demo` to use the new stack. M5 becomes legacy runtime removal and
  hardening, not the first runtime cutover. M4 can spill into server/client
  compatibility where needed, while proper buffer sync remains M4.1.
- Q11 accepted with validation split: existing simple automated render tests
  should pass because they are basic proof that the system works. After automated
  tests pass, the user will run real desktop and device demos and check for
  obvious regressions.

# Notes

- The agreed M4 plan is ambitious and may expose bumps during implementation.
  Prefer moving forward over over-preserving old abstractions, but record any
  hacky or temporary choices as they happen so a cleanup milestone can be planned
  immediately after.
