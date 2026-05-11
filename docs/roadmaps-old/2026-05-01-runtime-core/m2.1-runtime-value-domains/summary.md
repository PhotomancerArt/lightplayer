### What was built

- **`Production`** replaces the former produced-value envelope: `product: Versioned<RuntimeProduct>` plus `ProductionSource`, used by resolve session caching, trace/provenance, and `TickContext::resolve`.
- **`RuntimeProduct`** is the runtime domain enum: `Value(LpsValueF32)` for shader-portable payloads and `Render(RenderProductId)` for cheap handles into engine-managed storage.
- **Render-product boundary**: `RenderProductId`, sample request/result types, `RenderProductStore` on `Engine`, and deterministic test products for batch sampling — not real shader- or GPU-backed rendering.
- **Model boundary**: Removed `Texture2D` from `ModelValue` and `ModelType`; source still uses `SrcValueSpec::Texture`. `lps_value_f32_to_model_value` rejects `LpsValueF32::Texture2D` as non-portable.
- **`RuntimePropAccess`** documented as legacy/sync `LpsValueF32` bridge; resolver-driven domain results use `Production`.

### Decisions for future reference

#### Production as resolver/cache envelope

- **Decision:** Use `Production` (`Versioned<RuntimeProduct>` + source) everywhere the resolver session caches and traces resolved outputs — not legacy `ProducedValue` / `Versioned<LpsValueF32>` on that path.
- **Why:** Provenance stays one type while the carried payload can be data or an engine handle; caches stay cloneable handles, not blobs.
- **Rejected alternatives:** Pushing texture/render payloads into `ModelValue` or widening `LpsValueF32`-only envelopes for provenance-bearing resolution.
- **Revisit when:** Adding new domains beyond `Value`/`Render` (explicit enum variants + store/capabilities per domain).

#### RuntimeProduct `{Value, Render}`

- **Decision:** Split direct shader data (`Value`) from sampled visual products represented as `Render` handles.
- **Why:** Avoids overloading shader-value types with heavyweight or capability-backed payloads; aligns fixture-style sampling with engine-owned stores.
- **Rejected alternatives:** A single overloaded “runtime scalar” type for everything; expressing render outputs only through `ModelValue` or shader ABI texture descriptors.
- **Revisit when:** Real GPU textures, shaders-as-products, or streaming domains need richer identity and lifecycle rules.

#### Render products are store-backed handles

- **Decision:** The resolver caches `RuntimeProduct::Render(id)` only; payloads and sampling logic live behind `RenderProductStore` owned by `Engine`.
- **Why:** Keeps resolver entries small and deterministic; separates cache versioning from raster or buffer ownership.
- **Rejected alternatives:** Caching sampled pixels or descriptors inside the resolver map.
- **Revisit when:** Cross-engine sharing, eviction, or async production requires a clearer resource graph.

#### Texture wire transport

- **Decision:** Deferred. `LpsValueF32::Texture2D` remains shader/fixture ABI; wire and disk use portable scalars/recipes plus source-level texture specs, not nested texture pixels in `ModelValue`.
- **Why:** Reference and payload channels should separate identity from bulk data later; avoids duplicate pixel copies on repeated references.
- **Rejected alternatives:** Encoding live texture descriptors in `ModelValue` for transport.
- **Revisit when:** Defining MVP wire/update protocol for refs vs pixel uploads between host and firmware.
