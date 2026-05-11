# Phase 5 — Wire `TickContext` to Resolver, Bus, and Artifact

sub-agent: supervised
parallel: -

# Scope of phase

Turn the phase-1 `TickContext` shell into a useful runtime context that
delegates to the phase-4 resolver and exposes artifact/bus/frame helpers.

Out of scope:

- Do not cut over legacy `ProjectRuntime`.
- Do not expose mutable tree topology operations.
- Do not implement wire/view prop deltas.
- Do not add `ProjectDomain`.

# Code organization reminders

- Keep context code in `node/contexts.rs` unless it grows enough to warrant
  a small helper file.
- Prefer capability traits/facades over broad mutable references.
- Keep tests at the bottom.
- Avoid self-referential/lifetime-heavy designs where simpler ownership
  suffices.

# Sub-agent reminders

- Do not commit.
- This phase is nuanced; stop and report if the resolver/context borrow
  model is not working.
- Do not add unsafe.
- Do not suppress warnings or weaken tests.
- Do not change legacy runtime behavior.
- Report files changed, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md`, `00-design.md`, and phase 4 first.

Update:

- `lp-core/lpc-engine/src/node/contexts.rs`
- possibly `lp-core/lpc-engine/src/node/node.rs`
- possibly `lp-core/lpc-engine/src/resolver/*` if minor API adjustments are
  needed to make context delegation clean.

`TickContext` should expose:

- `node_id() -> NodeId`
- `frame_id() -> FrameId`
- `resolve(&mut self, prop: &PropPath) -> Result<&ResolvedSlot, ResolveError>`
  or an owned result if that is the resolver API chosen in phase 4.
- `changed_since(&self, prop: &PropPath, since: FrameId) -> bool`
- `artifact_changed_since(&self, since: FrameId) -> bool`
- bus helpers:
  - `bus_read(&self, channel: &ChannelName) -> Option<&LpsValueF32>` or
    equivalent
  - `bus_publish(...)` only if it can be supported without confusing
    ownership

The context needs access to:

- current node id
- current frame
- mutable current entry resolver cache
- current entry `SrcNodeConfig`
- current artifact content frame
- a resolver context facade from phase 4

If direct ownership is difficult, split data:

```rust
pub struct TickContext<'a, R> {
    node_id: NodeId,
    frame_id: FrameId,
    config: &'a SrcNodeConfig,
    resolver_cache: &'a mut ResolverCache,
    artifact_content_frame: FrameId,
    resolver: R,
}
```

where `R: ResolverContext`.

Keep the topology invariant: do not expose `&mut NodeTree`.

`DestroyCtx` and `MemPressureCtx` can remain smaller but should use the same
node/frame naming style.

Tests should cover:

- `TickContext::resolve` delegates to resolver and populates cache.
- `changed_since` reads cache frames correctly.
- `artifact_changed_since` compares against artifact content frame.
- a dummy `Node` can call `ctx.resolve` from `tick`.
- context does not expose tree mutation APIs (compile-time by omission; no
  special test needed).

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-engine
cargo test -p lpc-engine node::
cargo test -p lpc-engine resolver::
```
