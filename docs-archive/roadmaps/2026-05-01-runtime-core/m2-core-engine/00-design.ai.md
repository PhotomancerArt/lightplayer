# Scope of Work

Milestone 2 adds the first core runtime owner in `lpc-engine`: `Engine`. The
engine owns the runtime tree, binding registry, resolver, frame state, artifact
manager, and output capability for the new runtime spine.

The milestone proves demand-driven execution with dummy core nodes. It does not
port the concrete legacy shader/texture/fixture/output runtimes, replace
`LegacyProjectRuntime`, introduce render products, add async resolution, or add
full cross-frame dependency tracking.

# File Structure

```text
lp-core/lpc-engine/src/
├── lib.rs                         # UPDATE: export Engine, BindingRegistry, resolver types
├── engine/                        # NEW: core runtime owner
│   ├── mod.rs
│   ├── engine.rs                  # Engine owns tree, registry, resolver, frame state
│   ├── engine_error.rs            # Engine-level errors
│   └── test_support.rs            # TEST: builder and dummy legacy-shaped nodes
├── binding/                       # NEW: successor to the current bus value registry
│   ├── mod.rs
│   ├── binding_id.rs
│   ├── binding_entry.rs
│   ├── binding_registry.rs
│   └── binding_error.rs
├── bus/                           # UPDATE: keep bus nomenclature if useful, no value cache
│   └── ...                        # May re-export or shrink around binding registry concepts
├── resolver/                      # UPDATE: engine-owned resolution machinery
│   ├── mod.rs
│   ├── query_key.rs               # QueryKey::{Bus, NodeOutput, NodeInput}
│   ├── produced_value.rs          # Versioned<LpsValueF32> + source metadata
│   ├── resolve_trace.rs           # Active stack plus optional structured trace log
│   ├── resolver.rs                # Resolver owns same-frame cache and selection helpers
│   ├── resolve_session.rs         # Active per-frame/per-demand cycle-detecting session
│   └── resolver_cache.rs          # UPDATE: engine-level query cache
├── node/
│   └── contexts.rs                # UPDATE: TickContext resolves through ResolveSession
└── tree/
    └── node_entry.rs              # UPDATE: remove resolver_cache field
```

# Conceptual Architecture

```text
Engine
  owns NodeTree<Box<dyn Node>>
  owns BindingRegistry
  owns Resolver
  owns frame/time/artifacts/output capability

engine.tick()
  -> advance frame
  -> create ResolveSession for this frame
  -> tick demand roots with TickContext backed by ResolveSession
      -> node asks ctx.resolve(QueryKey::NodeInput { ... })
          -> ResolveSession updates trace stack and optional log events
          -> Resolver checks same-frame cache / active stack
          -> BindingRegistry finds binding candidates
          -> Resolver selects highest-priority binding
          -> ResolveSession asks Engine/ResolveHost to produce uncached values
              -> Engine ticks producer node if needed
              -> producer publishes Versioned<LpsValueF32>
          -> Resolver caches ProducedValue for QueryKey
  -> flush output-like side effects
```

# Main Components

## Engine

`Engine` is the central runtime owner for the new spine. It owns:

- `NodeTree<Box<dyn Node>>`
- `BindingRegistry`
- `Resolver`
- `ArtifactManager`
- frame id and frame timing
- output capability or output-like side-effect tracking

`Engine` drives demand roots synchronously. It sets up a `ResolveSession` for the
frame, constructs tick contexts for nodes, and mediates producer invocation when
the resolver misses the same-frame cache.

## Binding Registry

The current `Bus` concept becomes a central `BindingRegistry`. The bus remains
important nomenclature: it means bindings that use a label/channel as an input
or output. It is not the owner of resolved values.

Nodes register bindings when they come into existence and unregister them when
they leave. The registry validates kind/type consistency, priority conflicts,
equal-priority errors, and later multi-bus constraints. Binding entries have
identity and versioning so the UI can eventually receive a master bindings list
over the wire.

First-shape types:

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

## Resolver And ResolveSession

`Resolver` is owned by `Engine`. It owns the same-frame cache and binding
selection helpers. `ResolveSession` is the active per-frame/per-demand object
that carries the resolver trace, detects cycles, records optional trace log
events, and calls back to the engine host when an uncached query needs a
producer.

First query keys:

```rust
pub enum QueryKey {
    Bus(ChannelName),
    NodeOutput { node: NodeId, output: PropPath },
    NodeInput { node: NodeId, input: PropPath },
}
```

First produced value:

```rust
pub struct ProducedValue {
    pub value: Versioned<LpsValueF32>,
    pub source: ProductionSource,
}
```

Render-product keys are intentionally deferred to the render-product milestone.

## Resolver Trace

`ResolveSession` should support a first-class resolver trace from day one. The
trace combines cycle detection, optional structured logging, and value
provenance. It is not the full diagnostics surface; it is the correctness and
observability mechanism for one active resolution session.

The active stack is always enabled because it detects recursive/cyclic demand
and provides error context. Detailed log retention is optional and should be
disabled by default or cheap when unused. Tests can enable it and assert against
concise events.

Conceptual shape:

```rust
pub struct ResolveTrace {
    active_stack: Vec<QueryKey>,
    log_level: ResolveLogLevel,
    events: Vec<ResolveTraceEvent>,
}
```

Initial event vocabulary can be small:

```rust
pub enum ResolveTraceEvent {
    BeginQuery(QueryKey),
    CacheHit(QueryKey),
    SelectBinding { query: QueryKey, binding: BindingId },
    ProduceStart(QueryKey),
    ProduceEnd(QueryKey),
    CycleDetected { query: QueryKey },
    ResolveError { query: QueryKey },
}
```

Implementation may adjust names or payloads, but resolver tracing should remain
a first-class concept rather than ad hoc debug prints. Cycle detection should use
the active trace stack, not a separate duplicate mechanism.

This trace should be designed with future UI exposure in mind. Users and agents
need to answer "where did this value come from?" while debugging a project. M2
does not need to add the wire protocol, but trace/provenance events should be
structured enough that a later UI can show the binding, node output, cache hit,
or error path that produced a value.

## Producers

"Producer" is vocabulary for anything that can satisfy a query. M2 should not
add a separate public `Producer` trait yet. Instead, use a small host callback
shape, likely `ResolveHost`, so `ResolveSession` can ask `Engine` or a test fake
to produce uncached `QueryKey`s.

For node-backed production, the engine ticks the producer node at most once per
frame, then reads the requested output value as `Versioned<LpsValueF32>`.

## Dummy Validation Nodes

The runnable slice uses dummy core nodes, not concrete legacy adapters. Name the
dummy nodes around legacy roles where useful, such as `DummyShaderNode`,
`DummyFixtureNode`, and `DummyOutputNode`.

The target flow:

```text
DummyFixtureNode demand root
  -> resolves NodeInput
    -> resolves Bus("video_out")
      -> BindingRegistry selects binding
        -> resolves DummyShaderNode NodeOutput
```

This proves same-frame caching, recursive resolution, priority selection,
cycle detection, versioned values, and output-like side effects without coupling
M2 to the legacy runtime port.

## Test Support

M2 should include a test-only builder and small helper nodes so tests can define
graphs concisely. A filetest-style graph DSL is likely useful later, but should
wait until the engine setup language stabilizes.
