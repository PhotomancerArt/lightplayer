# M4: Core Runtime Cutover For MVP Demo

## Scope of Work

M4 is the clean-break cutover of the current MVP demo runtime path onto the core
engine. It ports the useful legacy texture/shader/fixture/output behavior into
first-class core `Node` implementations, wires authored projects into a core
`Engine`, and makes `just demo` use the new runtime stack.

This is allowed to be disruptive while in progress. `LegacyProjectRuntime`
remains available in git/worktrees as reference code, but M4 should not add a
temporary adapter layer around old runtime APIs.

In scope:

- Build current authored project layout into a core `Engine`.
- Introduce a `CoreProjectRuntime` owner for `Engine` plus runtime services.
- Port texture, shader/pattern, fixture, and output behavior into durable core
  nodes.
- Keep fixtures as demand roots and outputs as pushed sinks.
- Model shader/pattern output as render products, not generic runtime buffers.
- Keep runtime buffers for non-visual byte products such as fixture colors,
  output channels, protocol payloads, and compatibility snapshots.
- Rewire `lpa-server` / `just demo` to the new runtime path for the MVP project.
- Keep compatibility wire snapshots where needed until M4.1 adds proper
  buffer/render-product sync.

Out of scope:

- Temporary legacy adapter harness work; M3.3 is superseded.
- Full removal of `LegacyProjectRuntime`; M5 owns removal/hardening.
- Proper buffer-aware client sync; M4.1 owns that.
- Final render-product family beyond what the MVP path needs.
- Async/parallel scheduler execution.
- Any feature-gating or disabling of embedded shader compilation.

## File Structure

```text
lp-core/lpc-engine/src/
├── project_runtime/                         # NEW: core project runtime owner
│   ├── mod.rs
│   ├── core_project_runtime.rs              # owns Engine + services + sync facade
│   ├── project_loader.rs                    # current source layout -> Engine
│   ├── runtime_services.rs                  # graphics/output/product service surface
│   └── compatibility_projection.rs          # legacy wire snapshots until M4.1
├── nodes/
│   ├── core/                                # NEW: first-class core MVP nodes
│   │   ├── mod.rs
│   │   ├── texture_node.rs                  # texture metadata / product target
│   │   ├── shader_node.rs                   # shader/pattern producer
│   │   ├── fixture_node.rs                  # demand root, samples render product
│   │   └── output_node.rs                   # pushed sink / output provider flush
│   └── ...
├── render_product/
│   ├── render_product.rs                    # UPDATE: samplable product API if needed
│   ├── render_product_store.rs              # UPDATE: store product handles
│   └── texture_product.rs                   # NEW: current texture-backed product
├── engine/
│   └── engine.rs                            # UPDATE: tick runtime services access
└── node/
    └── contexts.rs                          # UPDATE: focused TickContext APIs

lp-app/lpa-server/src/
├── project.rs                               # UPDATE: own CoreProjectRuntime
├── project_manager.rs                       # UPDATE: load new runtime
└── server.rs                                # UPDATE: tick new runtime

lp-core/lpc-engine/tests/
├── scene_render.rs                          # UPDATE/PORT: new runtime path passes
├── scene_update.rs                          # UPDATE as needed
└── runtime_spine.rs                         # KEEP/UPDATE for lower-level coverage
```

Exact module names may move during implementation if the codebase points to a
better local convention, but the boundaries should remain: project runtime
owner, ported core nodes, render-product texture path, runtime services, server
wiring, and tests.

## Conceptual Architecture

```text
Current authored project
  /project.json + /src/*.kind/node.toml + shader files
          |
          v
CoreProjectRuntime::load
  - loads current authored layout
  - builds Engine NodeTree
  - installs bindings / demand roots
  - owns runtime services
          |
          v
Engine::tick
  fixture demand roots
      |
      v
FixtureNode
  resolve shader/pattern render products
  sample visual products
  push channel/color data into output sinks
      |
      v
OutputNode / Output service
  flush dirty output buffers through OutputProvider

ShaderNode / Pattern-like producer
  produces RuntimeProduct::Render(...)
  render product owns opaque samplable texture storage
  raw bytes are materialized only as an operation/snapshot
```

## Main Components

### CoreProjectRuntime

`CoreProjectRuntime` is the project-level owner that the server/demo path can
drive. It owns a core `Engine` plus services that do not belong inside generic
engine state: filesystem root, graphics, output provider, compatibility
projection, and source reload hooks.

It replaces the active demo role of `LegacyProjectRuntime` during M4. Full
legacy runtime deletion waits for M5.

### Runtime Services

Core nodes need a narrow services surface, not raw `&mut Engine` access and not
a copy of legacy `RenderContext`.

The services API should expose only what the node port needs:

- frame time;
- render/runtime buffer/product access;
- graphics/render-product creation and sampling operations;
- output sink handles and flush/write operations;
- artifact/source access where needed.

Use the old engine as inspiration, but design the API for the new core node
model.

### Render Products

Shader/pattern output is a render product. The public core path should pass an
opaque samplable product handle. The first implementation can be backed by the
current CPU/JIT texture allocation, but `LpsTextureBuf` should stay behind
`LpGraphics` / render-product internals.

Raw-byte materialization is a valid operation: GPU-backed hosts can copy a
texture to RAM when needed, while embedded paths may already have raw bytes. It
should not be the ownership model for visual products.

### Runtime Buffers

Runtime buffers remain for non-visual bytes:

- fixture color buffers;
- output channel buffers;
- raw protocol payloads;
- compatibility snapshots for legacy wire/view paths.

M4 may use compatibility snapshots temporarily. M4.1 replaces this with proper
buffer/render-product sync semantics.

### Fixture And Output Flow

Fixtures are demand roots. A fixture resolves the shader/pattern render products
it needs, samples them, and pushes channel/color data into output sinks.

Outputs are pushed data sinks, not pull-based demand roots. Outputs do not know
where data came from because fixtures own mapping information. This keeps room
for future many-to-many fixture -> output mapping.

### Validation

Existing simple automated render tests should pass; they are the basic proof
that the system works. After automated tests pass, manual desktop and device
demos will be used to catch obvious regressions.

Any hacky or temporary implementation choices should be recorded as they happen
for a cleanup milestone after M4.

## Phase Outline

1. Define CoreProjectRuntime and service surface          [sub-agent: yes,        model: gpt-5.5,  parallel: -]
2. Evolve render product texture/sampling API             [sub-agent: yes,        model: gpt-5.5,  parallel: -]
3. Build source project -> Engine loader                  [sub-agent: yes,        model: kimi-k2.5, parallel: -]
4. Port texture and shader producer nodes                 [sub-agent: yes,        model: gpt-5.5,  parallel: -]
5. Port fixture demand-root sampling node                 [sub-agent: yes,        model: gpt-5.5,  parallel: -]
6. Port output sink and flush path                        [sub-agent: yes,        model: kimi-k2.5, parallel: -]
7. Wire lpa-server / just demo to CoreProjectRuntime       [sub-agent: supervised, model: gpt-5.5,  parallel: -]
8. Port/repair scene render and update tests              [sub-agent: yes,        model: kimi-k2.5, parallel: -]
9. Cleanup, validation, and summary                       [sub-agent: supervised, model: gpt-5.5,  parallel: -]

These phases are intentionally mostly sequential. The shared API surface is
large, and merge conflicts would likely cost more than parallelism saves.
# M4: Core Runtime Cutover For MVP Demo Design

## Scope of Work

M4 makes a clean break from `LegacyProjectRuntime` for the MVP demo path. It
ports the useful legacy texture/shader/fixture/output behavior into first-class
core engine nodes, introduces a project runtime owner for the new stack, and
wires `lpa-server` / `just demo` to that runtime.

The branch may be broken during this work. The old runtime remains available as
reference code, but M4 should not add a temporary adapter layer whose main
purpose is preserving old runtime APIs.

In scope:

- `CoreProjectRuntime` in `lpc-engine`, owning `Engine` plus project services.
- A narrow runtime-services API for nodes that need graphics, output sinks,
  render products, buffers, frame time, and compatibility projection.
- Source project -> core `Engine` loading from the current authored layout.
- First-class core MVP nodes: texture, shader/pattern producer, fixture demand
  root, output sink.
- Render-product ownership for shader/pattern visual output.
- Runtime buffers for non-visual bytes such as fixture colors, output channels,
  raw protocol data, and compatibility snapshots.
- `lpa-server` / demo runtime cutover to the new project runtime.
- Passing automated scene render/update coverage and preserving embedded shader
  compilation.

Out of scope:

- Temporary legacy adapter harnesses.
- Full `LegacyProjectRuntime` removal; M5 owns cleanup.
- Proper buffer/render-product sync refs and client cache behavior; M4.1 owns
  that.
- Final render-product family design beyond the MVP product shape.
- Async or parallel scheduling.

## File Structure

```text
lp-core/lpc-engine/src/
├── project_runtime/                         # NEW: core runtime owner
│   ├── mod.rs
│   ├── core_project_runtime.rs
│   ├── project_loader.rs
│   ├── runtime_services.rs
│   └── compatibility_projection.rs
├── nodes/
│   └── core/                                # NEW: first-class MVP nodes
│       ├── mod.rs
│       ├── texture_node.rs
│       ├── shader_node.rs
│       ├── fixture_node.rs
│       └── output_node.rs
├── render_product/
│   ├── render_product.rs                    # UPDATE: samplable product API
│   ├── render_product_store.rs              # UPDATE as needed
│   └── texture_product.rs                   # NEW: texture-backed product
├── engine/
│   └── engine.rs                            # UPDATE: runtime services through tick
└── node/
    └── contexts.rs                          # UPDATE: focused TickContext APIs

lp-app/lpa-server/src/
├── project.rs                               # UPDATE: own CoreProjectRuntime
├── project_manager.rs                       # UPDATE: load new runtime
└── server.rs                                # UPDATE: tick new runtime

lp-core/lpc-engine/tests/
├── scene_render.rs                          # UPDATE: new runtime path
├── scene_update.rs                          # UPDATE as needed
└── runtime_spine.rs                         # KEEP/UPDATE lower-level coverage
```

## Conceptual Architecture

```text
Current authored project
  /project.json
  /src/*.texture/node.toml
  /src/*.shader/node.toml + main.glsl
  /src/*.fixture/node.toml
  /src/*.output/node.toml
        |
        v
CoreProjectRuntime::load(...)
  - owns Engine
  - owns graphics/output/runtime services
  - loads current project layout
  - builds NodeTree and demand roots
  - provides compatibility projection for legacy wire until M4.1
        |
        v
Engine::tick(...)
  demand roots = fixtures
        |
        v
FixtureNode
  - resolves shader/pattern render products
  - samples visual products
  - pushes channel/color bytes to output sinks
        |
        v
OutputNode / Output service
  - receives pushed channel data
  - flushes dirty outputs through OutputProvider

ShaderNode / Pattern-like producer
  - compiles GLSL through LpGraphics
  - produces RuntimeProduct::Render(...)
  - render product owns opaque samplable texture storage
  - raw bytes can be materialized as an operation/snapshot
```

## Main Components

### CoreProjectRuntime

`CoreProjectRuntime` is the production-facing runtime owner for M4. It owns the
core `Engine` and the services that do not belong inside bare engine spine:
filesystem root, graphics, output provider, compatibility projection, and reload
hooks. `lpa-server::Project` moves toward owning this type instead of
`LegacyProjectRuntime`.

### Runtime Services

Core nodes need focused access to runtime-owned resources without receiving a raw
`&mut Engine`. M4 introduces a narrow service surface for:

- frame time;
- render products and texture products;
- runtime buffers for non-visual byte payloads;
- graphics/shader compile and texture allocation;
- output sink handles and flush/write behavior.

The old `RenderContext` can inspire behavior, but the new API should be designed
for the core node model.

### Render Products

Shader/pattern output is a render product, not a runtime buffer. The public path
passes opaque, samplable product handles. The first implementation may wrap the
current CPU/JIT texture allocation. `LpsTextureBuf` should not leak through the
core node/product API, though raw bytes may be materialized when needed for
compatibility or host/GPU copies.

### Runtime Buffers

Runtime buffers remain for non-visual bytes: fixture colors, output channel
data, protocol payloads, and compatibility snapshots. They should not become the
authoritative owner of shader texture memory.

### Scheduling

Fixtures are demand roots. Outputs are pushed sinks because fixtures own mapping
information. The design should leave room for future many-to-many fixture ->
output mapping. Same-frame producer caching stays in the resolver/product path.

### Compatibility Projection

M4 may keep legacy wire/view compatibility through snapshots so clients and tests
keep functioning. Proper render/buffer refs, metadata/version exposure, client
caches, and diff policy are M4.1.

## Phase Outline

1. Define CoreProjectRuntime and service surface          [sub-agent: yes,        model: gpt-5.5,  parallel: -]
2. Evolve render product texture/sampling API             [sub-agent: yes,        model: gpt-5.5,  parallel: -]
3. Build source project -> Engine loader                  [sub-agent: yes,        model: kimi-k2.5, parallel: -]
4. Port texture and shader producer nodes                 [sub-agent: yes,        model: gpt-5.5,  parallel: -]
5. Port fixture demand-root sampling node                 [sub-agent: yes,        model: gpt-5.5,  parallel: -]
6. Port output sink and flush path                        [sub-agent: yes,        model: kimi-k2.5, parallel: -]
7. Wire lpa-server / just demo to CoreProjectRuntime      [sub-agent: supervised, model: gpt-5.5,  parallel: -]
8. Port/repair scene render and update tests              [sub-agent: yes,        model: kimi-k2.5, parallel: -]
9. Cleanup, validation, and summary                       [sub-agent: supervised, model: gpt-5.5,  parallel: -]
