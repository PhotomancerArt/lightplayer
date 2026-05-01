# Scope of Work

Milestone 2 creates the first real core runtime owner in `lpc-engine`: an
engine that owns the runtime tree, bus, artifact manager, frame state, output
capability, and engine-level same-frame resolution cache.

The plan should prove demand-driven execution without porting every legacy node
or settling the future render-product abstraction. The useful first slice is a
small core-engine flow where demand roots ask the engine/context for values,
the engine pulls producers as needed, and a demanded producer runs at most once
per frame.

Out of scope:

- Porting all legacy texture/shader/fixture/output runtimes to the new `Node`
  trait.
- Replacing `LegacyProjectRuntime` or changing its public behavior.
- Switching legacy source from JSON to TOML.
- Full render-product or visual-output abstraction.
- Async resolution or parallel scheduling.
- Cross-frame dependency tracking beyond preserving value versions.

# Starting State

At the start of M2, the codebase already had most spine pieces, but no owner
that drove them together:

- `lpc-engine/src/lib.rs` exports `NodeTree`, `Bus`, `ArtifactManager`,
  `ResolverCache`, `Node`, `TickContext`, and the legacy project runtime.
- `lpc-engine/src/node/node.rs` defines the new object-safe `Node` trait with
  `tick`, `destroy`, `handle_memory_pressure`, and `props`.
- `lpc-engine/src/node/contexts.rs` defined `TickContext`, but it was assembled
  manually by tests and still received a borrowed `ResolverCache` from the
  caller. M2 removed this old per-node cache path rather than preserving it as a
  compatibility layer.
- `lpc-engine/src/tree/node_entry.rs` still stored `resolver_cache:
  ResolverCache` on each `NodeEntry`; M2 moved the main resolver cache to the
  engine-owned resolution path.
- `lpc-engine/src/resolver/resolver.rs` resolves authored slots through
  overrides, artifact binding, and artifact defaults. Bus and node-prop lookups
  are read-only through `ResolverContext`; they do not pull producers.
- `lpc-engine/src/bus/bus.rs` stored channel metadata, writer
  identity, last value, and `last_writer_frame`. M2 should move away from this
  shape: the current `Bus` likely becomes a broader `BindingRegistry`, while
  the engine-owned resolver cache stores per-frame resolved values.
- `lpc-source/src/prop/src_binding.rs` says the authored bus is implicit:
  channels exist when bindings reference them, and direction is contextual from
  the slot role. There is no authored bus object in source files.
- `lpc-engine/src/artifact/artifact_manager.rs` provides handle/refcount/content
  frame tracking, but no `Engine` owns an artifact manager instance yet.
- `lpc-model/src/versioned.rs` provides `Versioned<T>` with a stored `FrameId`
  version. The older resolver path still returns `ResolvedSlot { value,
  changed_frame, source }`.
- `lpc-engine/src/legacy/project.rs` already has demand-like behavior:
  fixtures are tick roots, `RenderContext::get_texture` lazily renders shaders
  for a texture once per frame, and mutated outputs are flushed after fixture
  rendering. This is encoded in legacy-specific handles, `state_ver`, and a
  private `RenderContextImpl`, not in a general engine-owned resolver.

The implementation should stay in the existing `lpc-*` layout established by
M1. Reintroducing `lpl-*` crates or a generic domain layer is out of scope.

# Questions

## Confirmation-style

| # | Question | Context | Suggested answer |
| --- | --- | --- | --- |
| Q1 | Name the central type `Engine`? | Roadmap notes lean toward `Engine`; crate path already disambiguates as `lpc_engine::engine::Engine`. | Yes. |
| Q2 | Put the new owner under `lpc-engine/src/engine/` and re-export it from `lib.rs`? | Existing top-level modules are concept-oriented (`tree`, `bus`, `resolver`, `legacy_project`). | Yes. |
| Q3 | Keep the first engine contract synchronous? | `Node::tick` and `TickContext` are synchronous and ESP32 is the reference target. | Yes. |
| Q4 | Leave `LegacyProjectRuntime` behavior unchanged in M2? | M4/M5 are later roadmap milestones for legacy port and cutover. | Yes. |
| Q5 | Add an engine-owned cache for the core path while leaving `NodeEntry::resolver_cache` in place temporarily? | `NodeEntry::resolver_cache` does not appear to be needed by runtime code. | No; remove `NodeEntry::resolver_cache` now. |
| Q6 | Use `Versioned<LpsValueF32>` or a small wrapper around it for resolved scalar/vector values? | `Versioned<T>` is the roadmap primitive; `ResolvedSlot` still carries source metadata. | Use `Versioned<LpsValueF32>` directly for now; defer wrapper until the contract is clearer. |
| Q7 | Model cache keys explicitly as bus channel, node output, node input/slot, and texture/render product keys? | The milestone asks for the first query/cache key shape, even if not all are fully used. | Yes. |
| Q9 | Include cycle/re-entrant demand detection in the cache state machine, even with minimal diagnostics? | Same-frame caching needs to distinguish cached, in-progress, failed, and missing states. | Yes. |

Resolved answers:

- Q1-Q4, Q7, and Q9 are accepted as suggested.
- Q5 is changed: remove the old `NodeEntry::resolver_cache` now rather than
  keeping it temporarily.
- Q6 is narrowed: use `Versioned<LpsValueF32>` directly for M2; a richer wrapper
  can come later if source/key metadata needs to travel with values.
- Q8 is resolved at the cache/scheduling level: bus resolution is recursive
  normal resolution, values are cached in the engine resolver cache, and cycles
  must be detected.
- Q9 direction: the current `Bus` should become the engine-owned
  `BindingRegistry`. "Bus" remains a runtime concept/nomenclature for bindings
  that use a label/channel as their input or output, but it is not a separate
  value cache.
- Q11-Q13 direction: `Engine` owns a `Resolver`; demand runs inside a resolver
  context/session; the context calls back to engine-owned producers on cache
  misses; M2 query keys are `Bus`, `NodeOutput`, and `NodeInput`.
- Q10 direction: use dummy core nodes for the M2 runnable slice. Name them after
  legacy roles where useful, so the tests read semantically and the later
  transition is easier.
- Q14 direction: use a test-only builder for M2, but keep the door open for a
  text/filetest-style graph DSL once the language of engine setups stabilizes.
- Q15 direction: use `ResolveSession` for the active per-frame/per-demand
  object, `Resolver` for cache/selection state, and `ResolveHost` for the
  engine callback trait if needed.
- Resolver tracing is in scope from day one. Cycle detection and optional
  resolver logging should share one trace/stack mechanism: the active stack is
  always present for correctness, while detailed log events are optional.
- Resolver trace/provenance is likely UI-facing later. It should support
  answering "where did this value come from?" for users and agents debugging a
  project, even if M2 does not add the wire protocol yet.

## Discussion-style

## Q8: What exactly does the bus own?

The bus is conceptually close to a node or provider registry, but the resolved
value cache should live in the engine like the rest of the same-frame cache. The
bus should track what is bound to a channel and provide enough metadata for the
engine to choose a producer. It should not store the resolved frame value as the
authoritative cache.

Current direction to discuss:

- `Bus` tracks channel bindings/providers, not resolved values.
- Resolving a bus value goes through normal engine resolution and stores the
  result in the engine-owned resolver cache under a bus-channel query key.
- If multiple producers are bound to a channel, resolution picks the
  highest-priority binding that can produce a value.
- Equal-priority bindings may be an error, a deterministic tie-breaker, or a
  multi-writer policy; this needs an explicit decision.
- Error conditions should be part of the contract: no binding, all bindings
  fail to produce, conflicting equal priority, type/kind mismatch, and
  re-entrant/cyclic bus demand.

Answer:

- The resolved-value cache is not on the bus; it lives in the engine resolver
  cache.
- Resolving a bus channel is recursive normal resolution: choose the binding,
  then `resolve()` whatever input/source the binding points at.
- The engine must prevent cycles/re-entrant demand across bus and node-output
  resolution.
- Errors are acceptable and should be surfaced through engine diagnostics later:
  no binding, failed binding, equal-priority active bindings, kind/type
  mismatch, and recursive cycles are all diagnostic-worthy.
- Equal-priority active producers should be an error for M2.

## Q9: Is there a runtime bus object, or just a central binding registry?

The source model already treats the authored bus as implicit: channels exist
when slots bind to them, and there is no separate bus object in authored files.
Runtime still needs a way to discover "who can provide channel X" and with what
priority/kind. Earlier designs put writer state on `Bus`; the current direction
removes resolved values from the bus, which raises the question of whether
`Bus` remains useful as a runtime registry or whether the engine should own a
more general binding registry.

Possible shapes:

1. Keep `Bus` as the runtime channel binding registry.
   - `Bus` stores channel declarations/candidates, kind/type metadata, and
     priority.
   - The engine asks `Bus` for candidates and owns all resolution/cache state.
   - This keeps bus vocabulary localized but may overstate the bus as an
     object.
2. Replace/rename `Bus` with a central `BindingRegistry`.
   - The registry indexes all bindings by source and target: bus channels,
     node-output bindings, input/slot bindings, and later render-product
     bindings.
   - "Bus" becomes a naming convention for a class of binding keys, not an
     owner.
   - This may fit recursive resolution better, but risks becoming too abstract
     before enough binding kinds exist.
3. Keep bindings node-owned and build discovery indexes during engine init.
   - Node entries/artifacts remain the source of truth.
   - The engine maintains derived indexes for bus channels and other query
     keys.
   - This makes mutation/rebuild ownership explicit, but still needs a name for
     the derived index.

Answer:

- The current `Bus` should become the `BindingRegistry`.
- The bus is a convention for bindings that use a label/channel as their input
  or output, not the owner of resolved values.
- Nodes register their bindings when they come into existence and unregister
  them when they leave.
- Binding validation belongs in the registry: kind/type consistency, priority,
  equal-priority conflicts, and any future multi-bus constraints.
- The engine owns the actual value cache and refers to the registry when
  resolving queries.
- Keeping one central list of bindings is useful for UI/debugging and wire sync.
  Binding entries should therefore have versioning/change tracking so clients
  can receive binding-list updates and present a master bindings/debug view.
- Keep bus nomenclature around. It remains useful when discussing labeled
  channels, engine-to-engine sync, and future multiple-bus topologies.

## Q11: Should resolution live directly on `Engine`, or in an engine-owned `Resolver`?

The engine owns runtime state, but resolution has enough behavior to test
independently: query keys, cache entries, in-progress/cycle detection, binding
candidate selection, priority errors, and value versioning. A separate
`Resolver` owned by `Engine` may keep this logic focused and make unit tests
smaller. The risk is over-splitting the first implementation if `Resolver`
needs broad mutable access to `Engine` internals anyway.

Answer:

- `Resolver` should be an engine-owned component for cache and resolution state.
- Concrete value production happens through a per-frame/per-resolution context
  or session so cycle detection has one active query stack.
- The engine sets up a resolver context, ticks demand roots inside that
  context, and nodes ask the context for values.
- If a query is uncached, the context/resolver can call back into the engine for
  concrete producers. The engine owns nodes, and nodes are the producers of raw
  values.
- `Engine` still drives ticks and owns tree/artifacts/output; `Resolver` owns
  same-frame cache, query stack, and binding lookup/selection helpers.

## Q12: What is the first Producer abstraction?

Resolving an uncached query needs a way to invoke concrete producers without
making `Resolver` own the node tree. The producer may be a node output, a
bus-channel binding that recursively resolves another source, a literal/default
value, or later an external provider/render product. For M2, the important case
is a node-backed producer that can be ticked at most once per frame and then
read for a versioned output value.

Possible shapes:

1. `Engine` implements a resolver host trait.
   - `ResolverCtx` asks `Engine` to produce a `QueryKey`.
   - The trait methods can be unit-tested with fake hosts.
   - This avoids exposing node storage to `Resolver`.
2. Nodes implement a separate `Producer` trait.
   - `Engine` stores/registers producers and `Resolver` invokes them through
     trait objects.
   - This is explicit, but may duplicate the existing `Node` trait too early.
3. Producer is just a method on `Node`.
   - Extend `Node`/`TickContext` so `tick` publishes outputs and `props()`
     exposes raw values.
   - This is closest to the existing spine, but still needs engine mediation to
     avoid borrow/cycle issues.

Answer: use an engine/host trait for M2. Let `ResolverCtx` call a small
`produce(query, ctx)`-style host API implemented by `Engine` and by test fakes.
Keep `Producer` as vocabulary for "thing that can satisfy a query," but do not
add a separate public `Producer` trait until multiple non-node producer families
prove the need.

## Q13: What concrete types represent queries and productions?

Existing model types already cover most addressing:

- `NodeId` is the compact runtime node handle.
- `PropPath` addresses a property/slot on a node.
- `ChannelName` is the convention-only bus label.
- `NodePropSpec` is authored path + prop addressing, but engine runtime should
  resolve it to `NodeId + PropPath` before hot-path resolution where possible.
- `Versioned<T>` is the value + frame version primitive.

Answer:

```rust
pub enum QueryKey {
    Bus(ChannelName),
    NodeOutput { node: NodeId, output: PropPath },
    NodeInput { node: NodeId, input: PropPath },
}

pub struct ProducedValue {
    pub value: Versioned<LpsValueF32>,
    pub source: ProductionSource,
}

pub enum ProductionSource {
    Literal,
    Default,
    NodeOutput { node: NodeId, output: PropPath },
    BusBinding { binding: BindingId },
}
```

`QueryKey` is the cache/cycle key: if it is in progress, recursive resolution is
a cycle; if it is cached for the frame, return it without re-running the
producer. `ProducedValue` stays intentionally small for M2 and can grow into a
richer wrapper later if diagnostics/source stacks need to travel with values.

Bindings should be first-class entries rather than ad hoc maps:

```rust
pub struct BindingEntry {
    pub id: BindingId,
    pub source: BindingSource,
    pub target: BindingTarget,
    pub priority: BindingPriority,
    pub kind: Kind,
    pub version: FrameId,
    pub owner: NodeId,
}

pub enum BindingSource {
    Literal(SrcValueSpec),
    NodeOutput { node: NodeId, output: PropPath },
    BusChannel(ChannelName),
}

pub enum BindingTarget {
    NodeInput { node: NodeId, input: PropPath },
    NodeOutput { node: NodeId, output: PropPath },
    BusChannel(ChannelName),
}
```

For M2, `QueryKey` should stay to `Bus`, `NodeOutput`, and `NodeInput`. A future
render-product key is interesting, but should wait until the render-product
milestone. Bus providers plus node-output production are enough to prove the
resolver. The important design point is that bindings have identity and version,
so they can later be synced to the UI as a master binding/debug list.

## Q10: How much of the first demand-root flow should be legacy-backed?

Answer: use dummy core nodes for M2, not real legacy shader/fixture/output
adapters. The dummy nodes can be named around the legacy roles, for example
`DummyShaderNode`, `DummyFixtureNode`, and `DummyOutputNode`, so the demand flow
keeps semantic continuity with the later legacy transition.

The slice should prove:

- `Engine` owns tree, binding registry, resolver, frame state, and output-ish
  side effects.
- Demand roots tick inside a resolver context/session.
- A fixture-like demand root resolves `NodeInput`, which can resolve `Bus`,
  which selects a binding, which resolves a shader-like `NodeOutput`.
- The producing node runs at most once per frame.
- Recursive/cyclic resolution is detected.
- Returned values are `Versioned<LpsValueF32>`.
- Bindings have identity/versioning and can later become a UI/wire list.

## Q14: What test ergonomics should M2 provide?

The engine/resolver/binding-registry behavior will need several tests:
same-frame cache, bus priority selection, equal-priority errors, cycles,
node-output resolution, and binding registration/unregistration. These tests
will be painful if every case manually builds a tree, nodes, bindings, and
query paths from scratch.

Possible shapes:

1. Test-only builder pattern.
   - `EngineTestBuilder::new().shader(...).fixture(...).bind_bus(...).root(...)`
   - Stays normal Rust and is easy for sub-agents to extend.
   - Can live under `#[cfg(test)]` in the engine module.
2. Tiny test DSL.
   - More concise for graph-shaped setup.
   - Might be overkill before the engine API settles.
3. Production builder with test helpers layered on top.
   - Useful if project-loading will need a builder soon.
   - Risks hardening public construction API too early.

Answer: start with a test-only builder pattern and small helper node types for
M2. Keep it close to the tests and avoid a macro/DSL until repeated patterns
prove the exact shape. The likely future direction is a filetest-style DSL: a
block of text declares nodes/bindings/demand roots, and the test asserts either
resolved output or a structured error. Aim for M2 tests that describe the graph
in 5-10 lines rather than 80 lines of setup.

## Q15: Should the per-frame resolution object be called `ResolverCtx`?

Existing runtime names mostly use full `Context` in public-ish APIs:
`TickContext`, `RenderContext`, `NodeInitContext`, `DestroyCtx`, and
`MemPressureCtx`. The object in question is not just a passive context; it is a
per-frame/per-demand session that owns the active query stack and mediates cache
misses back to the engine host.

Possible names:

- `ResolverContext`: consistent with `TickContext`, but the name is already
  used by the current resolver facade trait in `resolver_context.rs`.
- `ResolveContext`: action-oriented and shorter; still clearly context-shaped.
- `ResolutionContext`: clearer noun, but a bit long.
- `ResolveSession`: emphasizes per-frame/per-demand active state and cycle
  stack; avoids collision with existing `ResolverContext`.
- `ResolverCtx`: concise, but less consistent with current public names and
  maybe too abbreviated for a central type.

Answer: use `ResolveSession` for the active per-frame object, and reserve
`Resolver` for the owner of cache/selection state. If a trait is needed for the
engine callback, use a host-style name such as `ResolveHost` rather than
overloading `Context`.

## Q16: Should resolver logging be merged with cycle detection?

Experience with register allocators suggests a structured execution log is
often the fastest way to validate and debug a complex demand system. Final
values alone do not show whether a producer was cached, which binding won, or
where a recursive query started.

Answer: merge the ideas into a first-class resolver trace on `ResolveSession`.
Do not make an append-only log the only source of truth for cycle detection:
cycle detection needs an efficient active query stack even when detailed logging
is disabled. The active stack and optional log should be owned by the same
`ResolveTrace`-style component so they cannot disagree.

Conceptually:

- Active stack: always enabled, used for cycle detection and error context.
- Detail log: optional, emitted from stack/cache/binding/producer transitions.
- Detail level: controls how much trace output is retained for tests/debugging.

This keeps the correctness path and observability path unified without making
cycle detection depend on storing a full debug log.

The trace is also the seed for UI-facing value provenance. Future UI/debug tools
should be able to ask why a value has its current value and see the bus binding,
node output, cache hit, or error path that produced it.

Initial optional event vocabulary can stay small:

- `BeginQuery(QueryKey)`
- `CacheHit(QueryKey)`
- `SelectBinding { query, binding }`
- `ProduceStart(QueryKey)`
- `ProduceEnd(QueryKey)`
- `CycleDetected { query }`
- `ResolveError { query }`

The exact event names can change during implementation, but the design should
make logging a deliberate concept rather than ad hoc debug prints.

## Q11: Should M2 create a public engine construction API or keep construction test-oriented?

The owner must exist as production code, but real project loading is still tied
to `LegacyProjectRuntime` and later migration milestones. A polished public
loader API may force decisions about source formats, artifact payloads, and
legacy node factories too early.

Suggested answer: create a real `Engine` type with small explicit constructors
and accessors for tests and later phases, but defer full project loading. Tests
can build a `NodeTree<Box<dyn Node>>`/engine fixture directly.

# Notes

- Validation should at least run `cargo test -p lpc-engine engine` or a more
  focused `cargo test -p lpc-engine` subset. Because this touches `lp-core/`,
  the final implementation should also run the ESP32 check required by the
  workspace rules when feasible.
