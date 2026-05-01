# Phase 4 — Implement Resolver Context and Cascade

sub-agent: supervised
parallel: -

# Scope of phase

Implement the engine-side consumed-slot resolver in
`lp-core/lpc-engine/src/resolver/`.

The resolver should support:

- per-instance override layer from `SrcNodeConfig.overrides`
- artifact bind layer
- artifact default layer
- `SrcBinding::Literal`
- `SrcBinding::Bus`
- `SrcBinding::NodeProp` dereference through target `RuntimePropAccess`
- default materialization to `LpsValueF32`
- `ResolverCache` population with `ResolvedSlot`

Out of scope:

- Do not implement wire `PropsChanged`.
- Do not update `lpc-view` prop cache.
- Do not wake pending target nodes.
- Do not implement graph cycle detection beyond avoiding recursion in this
  phase.
- Do not replace legacy runtime.

# Code organization reminders

- One concept per file.
- Keep the resolver context facade separate from the resolver algorithm.
- Keep tests focused and explicit.
- Avoid borrow-heavy APIs that make `TickContext` impossible to build later.
- Helpers live at the bottom of files.

# Sub-agent reminders

- Do not commit.
- This is nuanced; if borrow/API shape becomes ambiguous, stop and report.
- Do not add unsafe.
- Do not suppress warnings.
- Do not weaken tests.
- Do not broaden into domain/runtime cutover.
- Report files changed, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md` and `00-design.md` first.

Current relevant files:

- `lp-core/lpc-engine/src/resolver/resolver_cache.rs`
- `lp-core/lpc-engine/src/resolver/resolved_slot.rs`
- `lp-core/lpc-engine/src/resolver/resolve_source.rs`
- `lp-core/lpc-engine/src/resolver/binding_kind.rs`
- `lp-core/lpc-engine/src/bus/bus.rs`
- `lp-core/lpc-engine/src/prop/runtime_prop_access.rs`
- `lp-core/lpc-source/src/prop/src_binding.rs`
- `lp-core/lpc-source/src/prop/src_shape.rs`
- `lp-core/lpc-source/src/prop/src_value_spec.rs`

Create:

```text
lp-core/lpc-engine/src/resolver/resolver.rs
lp-core/lpc-engine/src/resolver/resolver_context.rs
```

Update `resolver/mod.rs` to export them.

Design a narrow resolver context facade. It should not own the full tree.
It needs enough access to:

- read bus values and bus changed frames
- read target node produced props for `NodeProp`
- materialize artifact slot bind/default for a path
- expose current frame

Suggested trait/facade shape:

```rust
pub trait ResolverContext {
    fn frame_id(&self) -> FrameId;
    fn bus_value(&self, channel: &ChannelName) -> Option<(&LpsValueF32, FrameId)>;
    fn target_prop(
        &self,
        node: &TreePath,
        prop: &PropPath,
    ) -> Option<(LpsValueF32, FrameId)>;
    fn artifact_binding(&self, prop: &PropPath) -> Option<&SrcBinding>;
    fn artifact_default(&self, prop: &PropPath) -> Option<LpsValueF32>;
}
```

Adjust details as needed for lifetimes and current source types. It is fine
for `artifact_default` to return owned `LpsValueF32` in M4.3.

Implement a resolver function along these lines:

```rust
pub fn resolve_slot<C: ResolverContext>(
    cache: &mut ResolverCache,
    config: &SrcNodeConfig,
    prop: &PropPath,
    ctx: &C,
) -> Result<&ResolvedSlot, ResolveError>
```

If returning `&ResolvedSlot` creates borrow pain, return an owned
`ResolvedSlot` and separately insert into cache. Prefer clarity over clever
borrowing.

Resolution priority:

1. If `config.overrides` contains `prop`, resolve that binding.
2. Else if artifact binding exists, resolve that binding.
3. Else materialize artifact default.

Binding behavior:

- `SrcBinding::Literal(v)`:
  - materialize to `ModelValue` if needed, then convert to `LpsValueF32`.
  - If no conversion helper exists yet, add a small focused helper in the
    resolver module for common scalar/vector variants used by tests. Do not
    create a broad conversion framework unless it is already available.
- `SrcBinding::Bus(ch)`:
  - read from `ctx.bus_value(ch)`.
  - if missing, fall through to artifact default for the same slot.
  - source should record `ResolveSource::Override(BindingKind::Bus)` or
    `ArtifactBind(BindingKind::Bus)` when successful.
- `SrcBinding::NodeProp(spec)`:
  - require `spec.target_namespace() == Some(PropNamespace::Outputs)`.
  - if target namespace is not outputs, return a `ResolveError` or cache
    failed/default according to the simplest consistent model.
  - use `ctx.target_prop`.
  - if missing, fall through to default.
- default:
  - `ResolveSource::Default`.

`ResolveSource` may need a new variant or use existing
`Override(BindingKind::NodeProp)` / `ArtifactBind(BindingKind::NodeProp)`.
Do not over-model dependency graphs yet.

Add a `ResolveError` type if one does not exist. Keep it focused and
string-carrying, similar to `NodeError`.

Tests should cover:

- override literal beats artifact binding/default.
- artifact binding beats default.
- missing bus falls through to default.
- bus read uses bus value and frame.
- node-prop reads target `RuntimePropAccess`.
- node-prop rejects non-outputs namespace.
- cache is populated with expected value/source/frame.

Use dummy resolver contexts in tests. Do not involve current legacy
`ProjectRuntime`.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-engine
cargo test -p lpc-engine resolver::
```
