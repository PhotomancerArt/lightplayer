# Runtime Core Notes

Working notes for the runtime core redesign. This is intentionally not a
formal implementation plan yet; use this file to collect architecture
questions, decisions, and constraints before writing phase plans.

## Why this roadmap exists

The M4.3 runtime spine created the parts of the new runtime:

- `Node`
- `TickContext`
- `NodeTree`
- `ArtifactManager`
- `ResolverCache`
- `Bus`

It did not create the owning runtime that drives those parts together. The
legacy runtime has now been renamed to `LegacyProjectRuntime`, which makes the
missing new owner explicit.

The new runtime needs to settle:

- what owns the bus, tree, artifacts, output provider, frame clock, and sync
  state;
- how nodes are initialized and allowed to mutate the tree;
- what it means to resolve a value;
- which nodes are ticked every frame vs only when demanded;
- how visual outputs should be represented when texture rendering is not the
  right cost model.

## Working approach

Roadmap this incrementally. Do not try to solve the whole runtime core in one
large plan.

Working sequence:

1. Reorganize around an update-in-place strategy.
2. Define the core engine/runtime owner and prove demand-driven scheduling.
3. Define the runtime value/domain envelope before render-like products are
   forced through scalar values.
4. Migrate the legacy shader -> fixture -> output source shape to TOML and
   `lpc-source`.
5. Port the legacy MVP runtime behavior onto the core engine.
6. Cut over to the core engine and retire the old runtime.
7. Revisit queryable render products once ordinary value resolution and demand
   scheduling are concrete.

The first milestone should undo the parts of the prior direction that pulled
legacy too far away from `lp-core`. Instead of maintaining separate legacy,
visual, and future rig crate families, prefer upgrading the existing runtime
concepts in place inside the `lpc-*` family and using modules for separation.

This is not final forever. It is the next step because it reduces indirection
while the core runtime contract is still being discovered.

See [`m1-update-in-place.md`](m1-update-in-place.md).

## Milestones

- [M1: Update-in-place runtime reorganization](m1-update-in-place.md)
- [M2: Core engine](m2-core-engine.md)
- [M2.1: Runtime value domains](m2.1-runtime-value-domains.md)
- [M3: Legacy source migration](m3-legacy-source-migration.md)
- [M4: Legacy node runtime port](m4-legacy-node-runtime-port.md)
- [M5: Core engine cutover](m5-core-engine-cutover.md)
- [M6: Render products (speculative)](m6-render-products.md)

## Current legacy flow

The legacy flow implemented in `lpc-engine` (`lpc_engine::legacy` plus
`LegacyProjectRuntime` in `legacy_project`) already has a partial pull model:

- `init_nodes` initializes legacy nodes in kind order:
  texture -> shader -> fixture -> output.
- `tick` advances the frame and renders all OK fixtures.
- A fixture render can request a texture through `RenderContext::get_texture`.
- `get_texture` calls `ensure_texture_rendered`, which lazily renders shaders
  targeting that texture only if the texture has not been rendered for the
  current frame.
- Fixture rendering writes to output buffers through `RenderContext::get_output`.
- Outputs are rendered/flushed after fixtures, but only outputs whose state was
  mutated this frame are visited.

The rough legacy dataflow is:

```text
fixture tick/render
  -> asks for texture
    -> lazy shader render for that texture
      -> shader writes texture buffer
  -> fixture samples texture
  -> fixture writes output buffer
output tick/render
  -> flush mutated output data
```

This is close to the desired pull behavior, but it is encoded through legacy
node kinds, texture handles, and render-context helper methods rather than a
general value-resolution contract.

## Profiling pressure

The ESP32-C6 profile from `examples-perf-fastmath --steady-render --p7-worley`
shows fixture rendering and texture sampling are major costs:

- `FixtureRuntime::render`: 1,152,304 self cycles, 19.6%.
- `FixtureRuntime::render`: 5,697,487 inclusive cycles, 96.8%.
- `RenderContextImpl::get_texture`: 4,001,408 inclusive cycles, 68.0%.
- `Rgba16Sampler::sample_pixel`: 288,768 self cycles, 4.9%.

This supports investigating a visual output abstraction that can serve both:

- full texture rendering, for debugging and GPU-oriented systems;
- direct sample queries, for fixtures that only need a sparse set of points.

## Candidate runtime owner

Working name options:

- `Engine`
- `EngineRuntime`
- `LightplayerEngine`

Initial lean: `Engine` for the central type, with module/type names doing the
disambiguation (`lpc_engine::engine::Engine`). If that is too generic in user
facing APIs, use `EngineRuntime`.

Likely ownership shape:

```rust
pub struct Engine {
    frame_id: FrameId,
    tree: NodeTree<Box<dyn Node>>,
    artifacts: ArtifactManager<...>,
    bus: Bus,
    output_provider: ...,
    // fs/watch, sync, time, memory pressure, graphics/JIT hooks
}
```

The important architectural point: `Bus` does not decide what work runs. The
engine does. The engine owns the active tick traversal, provides resolver
contexts, and can pull producers when a value is demanded.

## Lifecycle methods

Working lifecycle:

- `init` loads top-level project nodes, loads/acquires artifacts, instantiates
  initial roots, and lets nodes spawn declared children.
- `tick` advances the frame and drives the demand roots for that frame.
- `shutdown` destroys alive nodes and releases external resources.

Alternative names to consider:

- `load` / `advance` / `shutdown`
- `start` / `tick` / `stop`
- `open_project` / `tick` / `close_project`

Initial lean:

- `init` may be too generic if the runtime can later load multiple projects or
  reload top-level project definitions.
- `tick` is the right frame-advance name.
- `shutdown` is clearer than `destroy` for the whole engine.

## Tick roots and graph direction

The dataflow can be described as:

```text
input -> visual -> fixture -> output
```

But the runtime driving direction is not simply left-to-right.

Fixtures are logical endpoints because they consume visual/light data and
produce physical fixture output. Output devices are transport sinks that flush
mutated buffers; they are not usually the semantic source of demand.

Working tick model:

1. The engine identifies fixture-like demand roots for the frame.
2. The engine ticks each fixture.
3. The fixture resolves its input through `TickContext`.
4. Resolution follows literal, node-prop, bus, or default bindings.
5. If a bus or node-prop source has not produced a current-frame value,
   resolution pulls/ticks the producer.
6. The fixture writes transformed data into an output target.
7. The engine flushes mutated output devices.

Graph language:

- In dataflow terms, fixtures are sinks.
- In scheduler terms, fixtures are roots/drivers.
- Outputs are side-effect sinks flushed after demand roots have run.

Use "demand root" when discussing scheduling to avoid confusion.

## Value resolution contract

Open question: what exactly does resolving a value mean?

Current candidate:

Resolving a value is a demand operation that returns a frame-stamped value for a
specific `PropPath` on a specific node. It may:

- return a cached value if its dependencies are current enough;
- materialize a literal/default value;
- read a bus channel;
- pull/tick a node that produces a referenced output;
- invoke an external provider, such as audio input, time, or sensor data;
- record dependency/source information for cache invalidation and diagnostics.

Important rule:

Channel existence or binding existence does not imply producer work. Work runs
only when the active graph demands a value for the current frame.

## Concrete demand example

Sketch, using `$id` to indicate runtime node ids:

```text
fixture($fixture)
    input <- /bus/video_out
input($mode_selector) output -> /bus/mode
input($color_selector) output -> /bus/color
visual($playlist)
    output -> /bus/video_out
    param.activeInput <- /bus/mode
    child[0]: pattern($solid)
      param.color <- /bus/color
    child[1]: pattern($perlin)
```

Possible frame flow:

```text
engine.tick()
$fixture.tick()
ctx.resolve($fixture.input)
bus.resolve("video_out")
$playlist.tick()
ctx.resolve($playlist.activeInput)
bus.resolve("mode")
$mode_selector.tick()
// $playlist.activeInput -> 0
ctx.resolve($solid.output)
$solid.tick()
ctx.resolve($solid.param.color)
bus.resolve("color")
$color_selector.tick()
```

This is intentionally imperative. The runtime should not require node authors
to write a declarative mini build system. Nodes ask for values; the engine
ensures each demanded producer runs at most once per frame.

Caching layers:

1. **Engine per-frame cache:** mandatory. If a bus channel or node output has
   already been produced for frame N, return it. This prevents duplicate work
   within one tick.
2. **Cross-frame dependency cache:** optional and conservative. It should not
   require modeling every read in `tick` up front.
3. **Node-internal cache:** encouraged. Nodes can remember expensive internal
   products and decide whether to reuse them based on the values they care
   about.

Initial lean:

- Keep the core imperative: call `tick`, use `ctx.resolve`, produce outputs.
- Make same-frame caching an engine invariant.
- Add a small opt-in helper for cross-frame reuse, not a whole query language.

Possible opt-in shape:

```text
let active = ctx.resolve(active_input)?;
if ctx.unchanged_since_last_tick(&[active]) && self.cached_output.is_some() {
    ctx.publish_output(self.cached_output.clone());
    return Ok(());
}
```

The exact API should be better than this sketch, but the idea is that nodes can
ask for "did these resolved values change since my last successful output?" and
then choose a fast path. Complex nodes can manage their own staged caches.

This avoids forcing all nodes into a dynamic-dependency query system while
still giving common nodes an ergonomic way to skip expensive work.

## Engine-owned resolution

Emerging decision: all system-level value queries should route through the
engine.

Nodes should not directly ask child nodes for values and should not own the
main resolver cache. Instead, a node asks its context/engine for a produced
value:

```text
ctx.resolve_node_output(child_id, "outputs[0]")
ctx.resolve_bus("video_out")
ctx.resolve_input("inputs[0]")
```

The engine owns the per-frame cache of resolved bus values, node outputs, and
render products. This makes scheduling and duplicate-work avoidance a single
runtime concern:

- if a node output was already produced for this frame, return it;
- if it is currently being produced, detect re-entrant/cyclic demand;
- if it is stale or missing, tick/evaluate the producer;
- if it failed this frame, return the recorded failure/default behavior.

This direction likely replaces the M4.3 shape where `NodeEntry` owns
`resolver_cache: ResolverCache`. Node entries can still hold authored config,
artifact handles, status, lifecycle state, and node-owned private caches, but
the cross-node/system resolution cache belongs to the engine.

Node-owned caches remain valid for private implementation details such as
compiled shaders, selected child bookkeeping, texture buffers, render products,
or fixture sampling plans. The distinction is:

```text
engine caches resolved system values and scheduling state
nodes cache private implementation products
```

## Versioned values and stale checks

The simple internal caching story should be based on `Versioned<T>` from
`lpc-model`.

Working model:

- Every resolved input is represented as, or can be reduced to, a
  `Versioned<T>` value.
- `Versioned<T>` stores the current value plus the `FrameId` version from the
  last time the value was created or changed.
- Every cached private product records the version from the last time it was
  created.
- A node that wants to reuse a cached internal product compares the newest input
  version it used with the cached product's version.
- If any input version is newer than the product version, recreate the product.
- Otherwise, reuse the old product.

In pseudocode:

```text
let mode = ctx.resolve(mode_input)?      // Versioned<T>
let color = ctx.resolve(color_input)?    // Versioned<T>

let newest_input_version = max(mode.changed_frame(), color.changed_frame())
if self.cached_product.changed_frame() < newest_input_version {
    self.cached_product.set(frame_id, render(mode.value(), color.value()))
}

return self.cached_product.value()
```

The codebase now has the core primitive in
`lp-core/lpc-model/src/versioned.rs`:

```rust
pub struct Versioned<T> {
    value: T,
    version: FrameId,
}
```

Current API shape:

- `Versioned::new(frame_id, value)`
- `get`, `get_mut`, `set`, `mark_updated`
- `changed_frame`
- `value`, `into_value`

Conceptually, runtime-core design should use "version" language. The current
`changed_frame()` method remains an older compatibility/transition name for
reading the stored version; a future `version()` alias may make the API match
the concept more directly.

Other parts of the codebase still carry related frame/version concepts:

- `ResolvedSlot { value, changed_frame, source }`
- `RuntimePropAccess::get(...) -> Option<(LpsValueF32, FrameId)>`
- `NodeEntry` frame counters: `created_frame`, `change_frame`, `children_ver`
- artifact `content_frame`
- bus `last_writer_frame`

The key is to keep this simple and local. The engine owns system-level
resolution and provides versioned values through that path. Nodes decide which
versions matter for their own private caches, including compiled shaders,
selected child bookkeeping, texture buffers, render products, and fixture
sampling plans. We should not require node authors to model a full dependency
graph just to skip expensive work.

## Bus and providers

The bus should be a registry/cache plus producer lookup, not a background work
queue.

Possible model:

- `Bus` stores channel metadata, type/kind, last value, and last writer frame.
- A separate provider registry maps channels to producer capabilities.
- `ResolverContext::bus_value` is the pull point.
- If the channel value is stale for the frame, the engine invokes the provider
  or ticks the writer node.
- `Bus::publish` updates the cache after a producer has run.

This avoids running expensive sources such as `audio/0/fft` while no active
fixture/visual path asks for them.

## Async resolution

Question: should resolving values be `async`?

Arguments for:

- The dependency graph is logically parallel.
- Future hosts may have multiple cores or GPU/IO work.
- External sources may eventually have async readiness.

Arguments against:

- ESP32 target is currently single-threaded and `no_std`.
- Async in core traits can make object safety, allocation, and lifetimes harder.
- Most immediate demand resolution is synchronous CPU work.

Initial lean:

- Keep the core `Node`/`TickContext` contract synchronous for the first runtime
  core plan.
- Design the scheduler around explicit dependency/demand boundaries so parallel
  execution can be added above or beside the sync contract later.
- Do not bake thread-local assumptions into the value-resolution model.

## Visual output abstraction

The current legacy model renders visuals to textures, then fixtures sample
textures. That is useful for debugging and maps well to GPUs, but it wastes
work on ESP32 when fixtures only need sparse sample points.

Consider an abstraction such as `ImageSource`, `VisualSource`, or
`SampleSource` that can answer multiple query shapes:

```text
render full texture
sample one point
sample many fixture points
```

This lets simple shader visuals run directly for fixture sample points, while
more complex visuals such as stacks, blur, and feedback can still choose a
texture-backed representation.

Open questions:

- Is the visual output value itself a texture, an image source, or a more
  general render product?
- How does the resolver represent a value that is queryable rather than a plain
  `LpsValueF32`?
- Do fixtures ask for a `VisualSource` and then sample it, or does the engine
  expose fixture-aware batch sampling as the resolved operation?
- How do we preserve debugability when no full texture is produced?

## Domain split question

Prior direction had separate legacy, visual, and later rig domains. The new
concern is that this may create too much indirection:

- `lpl-{model,source,wire,engine}`
- `lpv-{model,source,wire,engine}`
- later rig equivalents

Alternative direction:

- Keep shared runtime/source/wire concepts in `lpc-{model,source,wire,engine}`.
- Use modules to separate legacy, visual, fixture, output, and rig concepts.
- Avoid a generic `ProjectDomain` abstraction unless a concrete duplication
  appears that modules cannot handle cleanly.

Initial lean:

- Fold more into `lpc-*` for the runtime-core work.
- Keep domain-like boundaries as modules and traits where they remove real
  duplication.
- Avoid making the central `Engine` generic over a domain until there is a clear
  second implementation that proves the abstraction.
- Treat the reintegration milestone as evidence-gathering: if updating the
  legacy flow in place is cleaner than a domain abstraction, keep going in that
  direction.

Milestone 1 (update-in-place, 2026-05-01 roadmap) **removed** the `lpl-model` and
`lpl-runtime` crates. Legacy configs live in `lpc-source::legacy`, legacy
wire/protocol payloads in `lpc-wire::legacy`, and concrete legacy runtimes in
`lpc-engine::legacy`; hook registration is removed. Further runtime-core work
stays in the `lpc-*` module layout rather than reintroducing parallel `lpl-*`
crates.

## MVP target

The first runtime-core target should be the old flow expressed in the new
paradigm:

- basic shader visual;
- fixture demand root;
- output flush;
- pull-based value/source resolution;
- legacy behavior preserved enough to compare against the old runtime.

This is a better validation target than building all future visual nodes first.
The legacy nodes can be treated as temporary implementations of the new
contracts, then replaced after the runtime flow is proven.

## After M1 direction

After the update-in-place refactor, there are two tempting next steps:

1. Build render products into the legacy engine first.
2. Build the new runtime owner/scheduler first.

Initial lean: build the runtime owner/scheduler first, but keep the first slice
small enough that it uses the legacy render products unchanged.

Why:

- Render products are important, but their shape depends on who owns demand,
  cache lifetime, mutation, and diagnostics.
- If render products land first inside the old engine, they may inherit legacy
  texture/output assumptions that the new scheduler is supposed to retire.
- A minimal `Engine` slice can preserve the old texture product while proving
  the more load-bearing contract: demand roots, pull resolution, frame-stamped
  outputs, and output flushing.

Suggested M2 target:

- introduce the new runtime owner shape (`Engine` / `EngineRuntime`);
- make it own `NodeTree`, `Bus`, artifacts, frame id, and output provider;
- prove demand-root scheduling and engine-owned resolution with a tiny slice;
- keep texture as the only render product for that slice;
- name the resolved output concept as a render product even if the only first
  implementation is texture-backed.

Then later milestones make the old flow real on the new engine before render
products become a primary focus:

- M3 migrates legacy source/config to TOML and `lpc-source`;
- M4 ports legacy shader -> fixture -> output behavior to the core engine;
- M5 cuts over and retires the old runtime path;
- M6 revisits render products, likely including texture-backed products,
  point/batch-sampled products, fixture-aware sampling, and debug/full-texture
  fallback.

## Major questions to resolve

These are the major design points to carry forward as the roadmap evolves:

1. What is the central type name: `Engine`, `EngineRuntime`, or
   `LightplayerEngine`?
2. Is `tick` driven by fixture-like demand roots, output devices, or an explicit
   scheduler root set?
3. What exact type does `resolve` return for ordinary scalar/vector slots vs
   queryable visual outputs?
4. Does value resolution stay synchronous for the first implementation?
5. Does the bus own producers, or does the engine own producers and use the bus
   only as registry/cache?
6. Are visual/legacy/rig domains separate runtime abstractions, or modules
   inside one runtime/source/wire/model family?
7. What tree mutations are allowed during `init` and `tick`, and through which
   context APIs?
8. How are output side effects represented: node output values, explicit output
   targets, or a separate output provider capability?
9. How much of the legacy engine should be adapted directly versus replaced by
   a new `Engine` immediately?
10. What is the smallest legacy-compatible slice that proves the new model:
    shader, fixture, output, bus, artifacts, or some narrower subset?

