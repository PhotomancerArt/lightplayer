# Phase 1: Model Render Product Value

## Scope Of Phase

Move graph render product identity into `lpc-model` and make it portable as an `LpValue`.

In scope:

- Add `lpc_model::RenderProduct`.
- Add `LpType::RenderProduct`.
- Add `LpValue::RenderProduct(RenderProduct)`.
- Add `ToLpValue` / `FromLpValue` support and a semantic `RenderProductSlot`.
- Update `lpc-engine` to import/re-export the model render product instead of owning it.

Out of scope:

- Materialized texture resources.
- Runtime state conversion.
- Wire/client feature work beyond serde/schema support from `LpValue`.

## Implementation Details

Suggested files:

- `lp-core/lpc-model/src/resource/render_product.rs`
- `lp-core/lpc-model/src/value/lp_type.rs`
- `lp-core/lpc-model/src/value/lp_value.rs`
- `lp-core/lpc-model/src/slots/render_product.rs`
- `lp-core/lpc-engine/src/render_product/mod.rs`
- `lp-core/lpc-engine/src/runtime_product/runtime_product.rs`

`RenderProduct` should be small, copyable, serde/schema compatible, and no-std:

```rust
pub struct RenderProduct {
    node: NodeId,
    output: u32,
}
```

Use stable public accessors rather than public fields unless existing model style suggests otherwise.

## Validate

```bash
cargo test -p lpc-model value::lp_value:: resource::
cargo check -p lpc-engine
```

