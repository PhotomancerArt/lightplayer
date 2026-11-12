### What was built

- `RuntimeBufferStore` on `Engine` (`runtime_buffers` / `runtime_buffers_mut`), sibling to `RenderProductStore`; entries are `Versioned<RuntimeBuffer>` with kind + metadata + bytes.
- Generic `RuntimeBufferId`; `RuntimeBuffer` / metadata enums cover texture, fixture colors, output channels, and raw payloads.
- `RuntimeProduct::Buffer(RuntimeBufferId)` plus accessors; `RuntimeProduct::try_value` / `value` reject `LpsValueF32::Texture2D` with `RuntimeProductError`.
- Resolver `Production::value` uses `try_value`; `SessionResolveError` maps `RuntimeProductError`.
- Crate exports for buffer and product types from `lib.rs`; focused unit tests in buffer store, `RuntimeProduct`, and engine round-trip.

### Decisions for future reference

#### Runtime buffers are sibling store-backed products

- **Decision:** Add `RuntimeBufferStore` beside `RenderProductStore`, not inside it.
- **Why:** Byte-heavy / non-sampleable payloads (fixture colors, output channels) do not fit the render sampling model; keeps domains clear for M4 adapters.
- **Rejected alternatives:** Extending `RenderProductStore` only; forcing all buffers through render traits.

#### Texture2D stays shader ABI, not RuntimeProduct::Value

- **Decision:** `LpsValueF32::Texture2D` is rejected by checked `RuntimeProduct` construction; texture-like runtime products use `RuntimeProduct::Buffer` (or render handles where sampling applies).
- **Why:** Shader ABI keeps `Texture2D`; product domain avoids hiding pixels in scalar values.
- **Rejected alternatives:** New scalar-only type for all of `LpsValueF32` in this milestone; allowing `Texture2D` in `RuntimeProduct::Value`.
