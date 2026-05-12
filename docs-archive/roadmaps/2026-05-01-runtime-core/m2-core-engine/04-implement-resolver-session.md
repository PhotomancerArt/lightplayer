## Scope of Phase

Implement the first working resolver/session layer on top of the binding
registry and resolver query/value/trace types.

This phase should make resolution behavior testable without the real `Engine`.
Use a `ResolveHost` trait and test fakes to prove cache hits, recursive bus
resolution, binding selection, equal-priority errors, and cycle detection.

Out of scope:

- Do not implement the final `Engine` owner.
- Do not wire real `NodeTree<Box<dyn Node>>` production yet.
- Do not add render-product resolution.
- Do not add UI/wire diagnostics.

Suggested sub-agent model: `gpt-5.5`.

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

Update or create these files:

```text
lp-core/lpc-engine/src/resolver/
├── resolver.rs                    # Resolver behavior
├── resolve_session.rs             # Active session API
├── resolve_host.rs                # Host callback trait, if separate
├── resolve_error.rs               # Structured errors for new path
└── mod.rs                         # Exports
```

Design constraints from `00-design.md`:

- `Resolver` owns the same-frame `ResolverCache`.
- `ResolveSession` owns/borrows the active `ResolveTrace`.
- `ResolveSession` resolves `QueryKey`.
- `ResolveSession` calls back to a host when an uncached query needs concrete
  production.
- The active stack in `ResolveTrace` is the cycle-detection mechanism.
- Detailed trace events are optional but should be easy to enable in tests.

Suggested API shape. Adjust names if needed for Rust borrow ergonomics, but keep
the concepts:

```rust
pub trait ResolveHost {
    fn produce(
        &mut self,
        query: &QueryKey,
        session: &mut ResolveSession<'_, Self>,
    ) -> Result<ProducedValue, ResolveError>
    where
        Self: Sized;
}

pub struct Resolver {
    cache: ResolverCache,
}

pub struct ResolveSession<'a, H: ResolveHost + ?Sized> {
    frame_id: FrameId,
    resolver: &'a mut Resolver,
    registry: &'a BindingRegistry,
    host: &'a mut H,
    trace: ResolveTrace,
}
```

If this exact self-referential callback shape is too awkward, use a safer
alternative, for example passing a small `ResolveRequest` enum to the host and
having `Engine` mediate the recursive call. Do not use unsafe.

Behavior to implement:

1. `resolve(QueryKey)` checks trace active stack; if active, return a cycle
   error and record a cycle trace event.
2. If the query is cached, return the cached `ProducedValue` clone/reference and
   record a cache-hit event.
3. For `QueryKey::Bus(channel)`, ask `BindingRegistry` for bus providers,
   select the highest priority provider, error on equal priority ambiguity, and
   recursively resolve the binding source:
   - `BindingSource::Literal(spec)` materializes a `ProducedValue` if reasonably
     available from existing helper code.
   - `BindingSource::NodeOutput { node, output }` resolves
     `QueryKey::NodeOutput { node, output }`.
   - `BindingSource::BusChannel(other)` resolves `QueryKey::Bus(other)`.
4. For `QueryKey::NodeInput { node, input }`, use the registry to find the
   binding for that input if present, otherwise call host/default behavior if
   the final API needs it. Keep this narrow if input binding indexing was not
   added in phase 2.
5. For `QueryKey::NodeOutput { .. }`, call `ResolveHost::produce`.
6. Cache successful produced values under the original query key.

Be conservative about literal/default materialization. If existing conversion
helpers are private and moving them would cause churn, this phase may support
literal `LpsValueF32` through a small test-only helper and leave full
`SrcValueSpec` materialization for later. Do not fake node-output production.

Tests to add:

- resolving the same node output twice in one frame calls the host producer once
- resolving a bus channel selects the highest-priority binding
- equal-priority providers for the same bus channel return a structured error
- bus-to-bus recursion resolves through both labels
- bus recursion cycle is detected by `ResolveTrace`
- trace events show begin/cache hit/select binding/produce start/end for a
  successful path when logging is enabled

## Validate

Run:

```bash
cargo test -p lpc-engine resolver
cargo test -p lpc-engine binding
```

If resolver changes affect existing `node` tests, also run:

```bash
cargo test -p lpc-engine node
```
