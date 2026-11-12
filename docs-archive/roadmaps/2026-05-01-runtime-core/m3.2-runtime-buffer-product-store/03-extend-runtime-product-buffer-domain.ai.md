# Phase 3: Extend RuntimeProduct Buffer Domain

## Scope of Phase

Extend `RuntimeProduct` so product-domain resolution can represent runtime
buffers and so texture ABI values cannot silently masquerade as scalar products.

In scope:

- Add `RuntimeProduct::Buffer(RuntimeBufferId)`.
- Add `RuntimeProduct::as_buffer`.
- Add a checked value constructor that rejects `LpsValueF32::Texture2D`.
- Update tests for value/render/buffer behavior and checked constructor errors.

Out of scope:

- Rewriting resolver call sites unless necessary for compile/test health.
- Removing `LpsValueF32::Texture2D` from shader ABI.
- Changing `Production` provenance or cache behavior beyond necessary constructor
  updates.
- Wire protocol changes.

## Code Organization Reminders

- Keep public entry points near the top.
- Keep helpers near the bottom.
- Keep tests concise and at the bottom of the file.
- Any temporary code should have a TODO comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If changing existing `RuntimeProduct::value` would create broad churn, make
  the minimal safe change and report the tradeoff.
- Report back: files changed, validation run, result, and any deviations.

## Implementation Details

Read first:

- `docs/roadmaps/2026-05-01-runtime-core/m3.2-runtime-buffer-product-store/00-design.md`
- `lp-core/lpc-engine/src/runtime_product/runtime_product.rs`
- `lp-core/lpc-engine/src/resolver/production.rs`
- `lp-core/lpc-engine/src/runtime_buffer/mod.rs`
- `lp-shader/lps-shared/src/lps_value_f32.rs`

Update:

- `lp-core/lpc-engine/src/runtime_product/runtime_product.rs`
- `lp-core/lpc-engine/src/resolver/production.rs` only if constructor changes
  require it.

Suggested type shape:

```rust
pub enum RuntimeProduct {
    Value(LpsValueF32),
    Render(RenderProductId),
    Buffer(RuntimeBufferId),
}
```

Add:

```rust
pub enum RuntimeProductError {
    Texture2dValueNotRuntimeProduct,
}
```

Add checked constructor:

```rust
pub fn try_value(value: LpsValueF32) -> Result<Self, RuntimeProductError> {
    match value {
        LpsValueF32::Texture2D(_) => Err(RuntimeProductError::Texture2dValueNotRuntimeProduct),
        other => Ok(Self::Value(other)),
    }
}
```

Existing `RuntimeProduct::value` currently returns `Self`. Prefer one of these
approaches:

1. If churn is small, make `RuntimeProduct::value` call `try_value` and return
   `Result<Self, RuntimeProductError>`, updating `Production::value` and tests.
2. If churn is broad, keep `RuntimeProduct::value` for existing scalar call
   sites but add docs saying it is for known-scalar values only, add
   `try_value`, and add a follow-up note in the phase report.

The design preference is to make misuse harder now, but do not expand the phase
into a large resolver refactor.

Add:

```rust
pub fn buffer(id: RuntimeBufferId) -> Self;
pub fn as_buffer(&self) -> Option<RuntimeBufferId>;
```

Tests:

- `runtime_product_buffer_helper_returns_id`
- checked value constructor accepts `F32` and rejects `Texture2D`
- `Render` and `Buffer` accessors do not cross domains
- if `Production::value` changes to return `Result`, update production tests
  accordingly.

Export `RuntimeProductError` from `runtime_product/mod.rs` and `lib.rs` if
needed.

## Validate

Run:

```bash
cargo test -p lpc-engine runtime_product
cargo test -p lpc-engine resolver::production
```
