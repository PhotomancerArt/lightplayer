## Scope of Phase

Add the concrete resolver data types for the new engine path: query keys,
produced values, production provenance, and resolver tracing.

This phase should define the vocabulary and unit-tested data structures only.
Later phases implement resolution behavior and engine integration.

Out of scope:

- Do not implement `Engine`.
- Do not implement full `Resolver::resolve` behavior.
- Do not wire `TickContext` through the new resolver yet.
- Do not add render-product query keys; render products are deferred.

Suggested sub-agent model: `kimi-k2.5`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public items and entry points near the top, helpers below them, and
  `#[cfg(test)] mod tests` at the bottom of Rust files.
- Keep related functionality grouped together.
- Any temporary code must have a TODO comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back what changed, what was validated, and any deviations from this
  phase plan.

## Implementation Details

Add or update files under `lp-core/lpc-engine/src/resolver/`:

```text
resolver/
├── mod.rs                         # UPDATE: export new types
├── query_key.rs                   # NEW
├── produced_value.rs              # NEW
├── resolve_trace.rs               # NEW
└── resolver_cache.rs              # UPDATE or keep old type behind new QueryKey cache API
```

Use these model types:

- `lpc_model::{ChannelName, FrameId, NodeId, PropPath, Versioned}`
- `lps_shared::LpsValueF32`
- `crate::binding::BindingId`

Define:

```rust
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum QueryKey {
    Bus(ChannelName),
    NodeOutput { node: NodeId, output: PropPath },
    NodeInput { node: NodeId, input: PropPath },
}

#[derive(Clone, Debug)]
pub struct ProducedValue {
    pub value: Versioned<LpsValueF32>,
    pub source: ProductionSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductionSource {
    Literal,
    Default,
    NodeOutput { node: NodeId, output: PropPath },
    BusBinding { binding: BindingId },
}
```

Implement an engine-level `ResolverCache` keyed by `QueryKey`, replacing the old
`PropPath`-only assumption if phase 1 has not already done so:

- `new()`
- `get(&QueryKey) -> Option<&ProducedValue>`
- `insert(QueryKey, ProducedValue) -> Option<ProducedValue>`
- `remove(&QueryKey) -> Option<ProducedValue>`
- `clear()`
- `iter()`
- `len()` / `is_empty()`

Add resolver trace types:

```rust
pub enum ResolveLogLevel {
    Off,
    Basic,
    Detail,
}

pub enum ResolveTraceEvent {
    BeginQuery(QueryKey),
    CacheHit(QueryKey),
    SelectBinding { query: QueryKey, binding: BindingId },
    ProduceStart(QueryKey),
    ProduceEnd(QueryKey),
    CycleDetected { query: QueryKey },
    ResolveError { query: QueryKey },
}

pub struct ResolveTrace {
    active_stack: Vec<QueryKey>,
    log_level: ResolveLogLevel,
    events: Vec<ResolveTraceEvent>,
}
```

`ResolveTrace` should support:

- `new(log_level)`
- `enter(query) -> Result<TraceGuard, ResolveTraceError>` or an equivalent safe
  API that guarantees stack pop
- `exit(query)` if not using a guard
- `is_active(&query) -> bool`
- `active_stack()`
- `events()`
- event recording that is cheap/no-op when `ResolveLogLevel::Off`

The active stack is correctness state for cycle detection and must work even
when logging is off. Optional events are for tests/debugging/UI provenance.

Tests to add:

- `QueryKey` can be used as a `BTreeMap` key
- `ProducedValue` stores `Versioned<LpsValueF32>` and source
- `ResolverCache` insert/get/cache-hit path
- `ResolveTrace` detects re-entering the same query as a cycle
- `ResolveTrace` pops active stack after a successful guarded scope
- `ResolveTrace` records no events at `Off` and records useful events at
  `Basic`/`Detail`

## Validate

Run:

```bash
cargo test -p lpc-engine resolver
```

If new exports depend on phase 2 binding types, also run:

```bash
cargo test -p lpc-engine binding
```
