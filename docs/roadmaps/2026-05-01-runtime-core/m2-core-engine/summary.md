### What was built

- Added `Engine` as the core runtime owner for the new demand-driven spine.
- Moved same-frame resolved-value caching out of `NodeEntry` and into the
  engine-owned resolver path.
- Added `BindingRegistry` for binding identity, versioning, bus-provider lookup,
  kind validation, and equal-priority provider rejection.
- Added `QueryKey`, `ProducedValue`, `ResolveSession`, `ResolveHost`, and
  `ResolveTrace` for recursive demand resolution, cycle detection, cache hits,
  and value-origin tracing.
- Routed `TickContext` resolution through a live `ResolveSession` via the
  `TickResolver` bridge.
- Added a test-only engine builder with dummy shader, fixture, and output nodes
  to validate demand roots, bus selection, same-frame caching, recursive
  resolution, cycles, and versioned values without porting legacy runtimes.

### Decisions for future reference

#### Binding Registry Keeps Bus Nomenclature

- **Decision:** Replace the old runtime bus value-cache shape with
  `BindingRegistry`, while keeping bus/channel vocabulary for labeled bindings.
- **Why:** Source treats bus channels as implicit labels, and the engine needs a
  central binding list for lookup, validation, debugging, and future wire sync.
- **Rejected alternatives:** Keep bus as the resolved-value owner; keep bindings
  node-owned and derive lookup indexes only during engine initialization.
- **Revisit when:** Multiple bus topologies or external binding sources need a
  richer ownership model than the current central registry.

#### ResolveTrace Owns Cycle Detection Context

- **Decision:** Use `ResolveTrace` for the active query stack and optional
  structured trace events.
- **Why:** Cycle detection must always be present, and value-provenance logging
  should not become a separate mechanism that can disagree with the correctness
  path.
- **Rejected alternatives:** Add ad hoc debug logging around resolver calls; use
  a separate cycle stack unrelated to diagnostic trace events.
- **Revisit when:** The UI diagnostics surface needs additional event payloads,
  filtering, or persisted trace snapshots.

#### ResolveSession Is The Active Resolution Object

- **Decision:** Keep `Resolver` as the owner of same-frame cache and selection
  state, and use `ResolveSession` as the active per-frame/per-demand object that
  calls a `ResolveHost` on cache misses.
- **Why:** The session owns temporary stack/trace state while allowing the engine
  to keep ownership of nodes, artifacts, bindings, and frame state.
- **Rejected alternatives:** Put all resolution methods directly on `Engine`;
  introduce a public `Producer` trait before non-node producer families exist.
- **Revisit when:** Runtime producers expand beyond node outputs, literals, bus
  bindings, and defaults enough to justify a public producer abstraction.

#### Dummy Core Slice Before Legacy Port

- **Decision:** Prove M2 with test-only dummy shader, fixture, and output nodes
  instead of adapting concrete legacy runtimes.
- **Why:** The milestone needed to validate engine ownership, demand resolution,
  cache behavior, and cycle handling without also changing source loading or
  legacy runtime behavior.
- **Rejected alternatives:** Port shader/fixture/output runtimes immediately;
  build a text filetest DSL before the engine setup language stabilized.
- **Revisit when:** M4 starts porting concrete legacy node behavior onto the core
  engine, or repeated Rust test builder patterns justify a filetest-style DSL.
