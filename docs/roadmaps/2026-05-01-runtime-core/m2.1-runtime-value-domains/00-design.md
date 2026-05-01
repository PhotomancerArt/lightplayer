# Scope of Work

Milestone 2.1 introduces runtime value domains for the core engine. The new
engine-owned resolution path should return a produced value whose payload can be
directly carried data or a handle into engine-managed render-product storage.

This milestone bridges M2's `Engine` / `Resolver` / `ProducedValue` work and the
M3/M4 legacy source/runtime migration. It prevents texture and future
sampleable visual outputs from being forced through `ModelValue` or
`LpsValueF32` as the only representation.

M2.1 includes:

- a `RuntimeValue` payload enum for produced values;
- `RuntimeValue::Data(LpsValueF32)` for directly carried GLSL-compatible data;
- `RuntimeValue::RenderProduct(RenderProductId)` for engine-managed visual
  products;
- minimal render-product sampling wiring, with test products only;
- removal of model-side `Texture2D` value/type variants if call sites can be
  updated cleanly;
- scalar/data compatibility helpers so M2 behavior remains readable.

M2.1 does not include:

- real shader-backed render products;
- texture-backed product storage;
- GPU resource ownership;
- texture pixel wire transport;
- the full legacy shader/fixture/output runtime port;
- async or parallel scheduling.

# File Structure

```text
lp-core/
├── lpc-model/src/
│   ├── lib.rs                         # UPDATE: export domain descriptors if added
│   └── prop/
│       ├── mod.rs                     # UPDATE: exports
│       ├── model_type.rs              # UPDATE: remove Texture2D variant
│       ├── model_value.rs             # UPDATE: remove Texture2D variant
│       └── value_domain.rs            # NEW: portable ValueDomain / ProducedType vocabulary if needed
└── lpc-engine/src/
    ├── lib.rs                         # UPDATE: export RuntimeValue / render-product types
    ├── runtime_value/                 # NEW: engine-owned produced payloads
    │   ├── mod.rs
    │   └── runtime_value.rs           # RuntimeValue::{Data, RenderProduct}
    ├── render_product/                # NEW: minimal product handle + sampling boundary
    │   ├── mod.rs
    │   ├── render_product_id.rs       # RenderProductId
    │   ├── sample_request.rs          # RenderSamplePoint / RenderSampleBatch
    │   ├── sample_result.rs           # RenderSample / RenderSampleBatchResult
    │   └── render_product_store.rs    # Testable engine-managed product store
    ├── resolver/
    │   ├── mod.rs                     # UPDATE: exports
    │   ├── produced_value.rs          # UPDATE: Versioned<RuntimeValue>
    │   ├── resolve_session.rs         # UPDATE: literals produce RuntimeValue::Data
    │   ├── resolver.rs                # UPDATE: model conversion produces RuntimeValue::Data
    │   └── resolver_cache.rs          # VERIFY: cache remains domain-agnostic
    ├── engine/
    │   └── engine.rs                  # UPDATE: own RenderProductStore if needed for sample wiring tests
    ├── node/
    │   └── contexts.rs                # UPDATE: tests use RuntimeValue helpers
    ├── prop/
    │   └── runtime_prop_access.rs     # DECIDE: keep data-only bridge or update new engine path
    └── wire_bridge/
        ├── lps_value_to_model_value.rs # UPDATE: remove ModelValue::Texture2D mapping
        └── model_type_to_lps_type.rs   # UPDATE: remove ModelType::Texture2D mapping
```

# Conceptual Architecture

```text
ResolveSession
  resolves QueryKey
    -> Resolver cache lookup
    -> BindingRegistry/source selection
    -> ResolveHost production if needed
    -> ProducedValue

ProducedValue
  source: ProductionSource
  value: Versioned<RuntimeValue>

RuntimeValue
  Data(LpsValueF32)
    directly carried GLSL-compatible runtime data:
    scalar, vector, matrix, array, struct, or shader ABI value

  RenderProduct(RenderProductId)
    small cloneable handle into engine-managed product storage

Engine
  owns Resolver
  owns BindingRegistry
  owns NodeTree
  owns RenderProductStore

RenderProductStore
  RenderProductId -> product implementation
  sample_batch(RenderProductId, RenderSampleBatch) -> RenderSampleBatchResult
```

The resolver cache stores small cloneable `ProducedValue`s. Heavy data,
resource-backed objects, GPU handles, texture buffers, sampled products, and
future stream-like products are owned outside the cache by the engine or by
node/product-private storage.

# Main Components

## RuntimeValue

`RuntimeValue` is the engine-time payload stored inside `ProducedValue`.

```rust
pub enum RuntimeValue {
    Data(LpsValueF32),
    RenderProduct(RenderProductId),
}
```

`Data` means directly carried GLSL-compatible runtime data. The name is broad,
but in this milestone it explicitly means "the value is carried in the
envelope." It does not mean every future data domain must be represented by
this variant.

`RenderProduct` means the produced value is a handle into engine-managed visual
product storage. The handle is cheap to clone and safe to cache. The product
itself owns or references whatever is needed to sample or later render a full
texture.

Helpers should keep existing scalar/data code readable:

```rust
impl RuntimeValue {
    pub fn data(value: LpsValueF32) -> Self;
    pub fn render_product(id: RenderProductId) -> Self;
    pub fn as_data(&self) -> Option<&LpsValueF32>;
    pub fn as_render_product(&self) -> Option<RenderProductId>;
}

impl ProducedValue {
    pub fn data(value: Versioned<LpsValueF32>, source: ProductionSource) -> Self;
}
```

## ProducedValue

`ProducedValue` remains the resolver/cache/provenance envelope:

```rust
pub struct ProducedValue {
    pub value: Versioned<RuntimeValue>,
    pub source: ProductionSource,
}
```

`ProductionSource::Literal` remains provenance. It should not be confused with a
value domain. A literal source can produce `RuntimeValue::Data`, and a node
output can also produce `RuntimeValue::Data`.

## Render Products

M2.1 adds render-product wiring, not real rendering. The goal is to establish
the core shape: a resolved value can be a render-product handle, and a consumer
can ask the engine/product store for sampled values.

First shape:

```rust
pub struct RenderProductId(u32);

pub struct RenderSamplePoint {
    pub x: f32,
    pub y: f32,
}

pub struct RenderSampleBatch {
    pub points: Vec<RenderSamplePoint>,
}

pub struct RenderSample {
    pub color: [f32; 4],
}
```

The exact sample coordinate type may change during implementation, but it should
stay small and fixture-oriented. Tests can register deterministic fake products
that return fixed or coordinate-derived colors.

Real shader-backed products, texture products, GPU resources, fallback previews,
and sampling optimizations remain future work.

## Model And Source Boundary

`lpc-model` should not own engine resource identity. It can own portable domain
or type descriptors if useful, but runtime handles belong in `lpc-engine`.

`ModelValue::Texture2D` and `ModelType::Texture2D` should be removed if the
call-site cleanup is tractable in M2.1. Texture-like authoring recipes should
remain source-level concepts (`SrcValueSpec::Texture` / `SrcTextureSpec`) until
the engine materializes them into runtime domains.

`LpsValueF32::Texture2D` is out of scope. It remains a shader ABI/runtime
compatibility shape for now.

## Runtime Prop Access

`RuntimePropAccess` currently exposes node-produced fields as `LpsValueF32`.
M2.1 should avoid letting that trait define the future runtime-domain boundary.
There are two acceptable outcomes:

- update the new engine-facing path to expose `RuntimeValue`; or
- explicitly leave `RuntimePropAccess` as a legacy/data-only bridge and keep
  render products on the new produced-value path.

The implementation should choose the smallest path that keeps M2 behavior
working and prevents new render-product assumptions from flowing through
`LpsValueF32`.

## Wire Boundary

M2.1 is not the texture wire-transport milestone. It should remove the old
`ModelValue::Texture2D` workaround if possible and avoid defining pixel payload
transport.

The future wire model should send texture/render-product references separately
from texture payload updates. Multiple references to the same texture should
share one pixel resource instead of duplicating data.
