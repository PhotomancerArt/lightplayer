# Phase 2: Convert Production To RuntimeProduct

sub-agent: yes
model: kimi-k2.5
parallel: -

## Scope of phase

Change the engine-owned resolver production envelope from carrying
`Versioned<LpsValueF32>` to carrying `Versioned<RuntimeProduct>`.

The repository already has the mechanical rename from `ProducedValue` to
`Production`; build on that current state.

In scope:

- Update `lpc-engine/src/resolver/production.rs`.
- Update resolver/session/cache/node tests and call sites.
- Add compatibility helpers so direct value tests remain readable.
- Keep ordinary scalar/vector value resolution behavior unchanged.

Out of scope:

- Do not add `RenderProductStore` behavior.
- Do not remove `ModelValue::Texture2D`.
- Do not change legacy `ResolvedSlot` unless required by this phase.
- Do not redesign `RuntimePropAccess`.
- Do not implement real render products.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public types and impls near the top; helpers below them.
- Place tests at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]`; fix warnings.
- Do not disable, skip, or weaken existing tests.
- If blocked or ambiguous, stop and report instead of improvising.
- Report back: files changed, validation run, result, and deviations.

## Implementation Details

Update `lp-core/lpc-engine/src/resolver/production.rs` from:

```rust
pub struct Production {
    pub value: Versioned<LpsValueF32>,
    pub source: ProductionSource,
}
```

to:

```rust
pub struct Production {
    pub product: Versioned<RuntimeProduct>,
    pub source: ProductionSource,
}
```

Add helpers:

```rust
impl Production {
    pub fn new(product: Versioned<RuntimeProduct>, source: ProductionSource) -> Self;

    pub fn value(value: Versioned<LpsValueF32>, source: ProductionSource) -> Self {
        let frame = value.changed_frame();
        Self::new(Versioned::new(frame, RuntimeProduct::value(value.into_value())), source)
    }

    pub fn as_value(&self) -> Option<&LpsValueF32>;
}
```

`Versioned<T>` may not have `map`; if not, construct the new version manually:

```rust
let frame = value.changed_frame();
let product = RuntimeProduct::value(value.into_value());
Versioned::new(frame, product)
```

Make sure `Production::value(...)` is the preferred helper for literal/default
and node-output value productions.

Update call sites:

- `lpc-engine/src/resolver/resolve_session.rs`
- `lpc-engine/src/engine/engine.rs`
- `lpc-engine/src/node/contexts.rs`
- `lpc-engine/src/resolver/resolver_cache.rs`
- `lpc-engine/src/resolver/tick_resolver.rs`
- tests under `lpc-engine/src/**` and `lpc-engine/tests/runtime_spine.rs`

Patterns like this:

```rust
pv.value.get().eq(&LpsValueF32::F32(1.0))
```

should become one of:

```rust
pv.as_value().expect("value").eq(&LpsValueF32::F32(1.0))
```

or:

```rust
pv.product.get().as_value().expect("value").eq(&LpsValueF32::F32(1.0))
```

Pick the style that keeps tests clear.

Literal materialization in `ResolveSession` should wrap materialized
`LpsValueF32` values as `RuntimeProduct::Value` via `Production::value(...)`.

When `EngineResolveHost` reads a node output through `RuntimePropAccess`, it
still gets `LpsValueF32`; wrap that as `RuntimeProduct::Value`.

Suggested tests:

- Update existing production tests to assert `Production.product` contains
  `RuntimeProduct::Value`.
- Add a test that `Production::value(...)` preserves the frame version.
- Update existing same-frame cache tests to ensure cached products remain
  unchanged across repeated value resolves.

## Validate

Run:

```bash
cargo test -p lpc-engine
```
