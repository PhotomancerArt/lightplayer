# Scope of Work

M3.2 defines the minimal runtime-owned product/buffer storage pattern needed
before M4 ports legacy shader -> fixture -> output behavior onto the core
engine.

The immediate goal is not a final render/transport system. The goal is to stop
M4 from accidentally treating texture pixels, fixture lamp colors, and output
channel bytes as ordinary scalar values or authoritative wire state.

M3.2 should answer:

- where texture-like and raw/color buffer data lives at runtime;
- what handles identify runtime-owned products/buffers;
- how `RuntimeProduct` represents texture/render-like payloads without hiding
  them inside `Value(LpsValueF32::Texture2D)`;
- how legacy `NodeState` snapshots relate to authoritative runtime storage;
- what minimal API M3.3/M4 adapter nodes can publish and consume.

In scope:

- Define the minimal product/buffer identity model: IDs, metadata, versions, and
  ownership.
- Extend or complement `RenderProductStore` so texture-backed compatibility can
  be represented.
- Add focused tests for product/buffer IDs, metadata, versioning, and lookup.
- Add guardrails so texture-like payloads are not represented as
  `RuntimeProduct::Value(LpsValueF32::Texture2D)` on the core product path.
- Document how `SyncProjection` should get legacy compatibility snapshots from
  runtime-owned stores.

Out of scope:

- Full binary transport, compression, scaling, throttling, chunking, or diff
  algorithms.
- Replacing legacy `GetChanges` / `NodeState` snapshots.
- Porting legacy runtime nodes onto core `Engine`.
- GPU-backed desktop resources.
- Cross-engine sharing, eviction policy, or async production.
- Removing `LpsValueF32::Texture2D` from shader ABI paths.

# Current State

## Runtime Product Domain

`RuntimeProduct` currently has two variants:

- `Value(LpsValueF32)` for direct shader-compatible scalar/vector/struct data.
- `Render(RenderProductId)` for handles into `RenderProductStore`.

`Production` wraps a `Versioned<RuntimeProduct>` plus `ProductionSource`, and
`ResolveSession` / `TickContext::resolve` are the product-domain resolution path.

Important boundary from M2.1:

- `ModelValue` and `ModelType` no longer include texture variants.
- `lps_value_f32_to_model_value` rejects `LpsValueF32::Texture2D`.
- `LpsValueF32::Texture2D` remains shader/fixture ABI in `lp-shader`.

Open issue:

- `RuntimeProduct::Value(LpsValueF32)` can technically hold
  `LpsValueF32::Texture2D`, even though texture-like runtime products should
  move toward handles.

## Render Product Store

`RenderProductStore` is engine-owned and maps `RenderProductId` to
`Box<dyn RenderProduct>`.

Current API:

- `insert(Box<dyn RenderProduct>) -> RenderProductId`
- `sample_batch(id, &RenderSampleBatch) -> RenderSampleBatchResult`

Current render product trait:

- `RenderProduct::sample_batch(...)`

Current limitations:

- no metadata (width, height, format, byte length, kind);
- no version/frame tracking;
- no raw byte snapshot API;
- no typed texture/color/output buffer identity;
- no removal/lifecycle API;
- no way to distinguish render products from raw buffers except by trait use.

## Legacy Heavy State Snapshots

M3.1 intentionally kept heavy fields in legacy wire state as compatibility
snapshots:

- `TextureState`
  - `texture_data: Versioned<Vec<u8>>`
  - `width: Versioned<u32>`
  - `height: Versioned<u32>`
  - `format: Versioned<TextureFormat>`
- `FixtureState`
  - `lamp_colors: Versioned<Vec<u8>>`
  - `mapping_cells: Versioned<Vec<MappingCell>>`
  - `texture_handle`, `output_handle`
- `OutputState`
  - `channel_data: Versioned<Vec<u8>>`

These are base64 JSON snapshots on the legacy wire path. They are not the
long-term authoritative runtime storage model.

## Legacy Runtime Buffers

Current legacy runtimes still own buffers internally:

- `TextureRuntime` owns `TextureState`, but `texture_data` is currently emptied
  with a TODO that live pixels should come from an upstream shader buffer.
- `FixtureRuntime` owns fixture state, mapping data, and lamp colors.
- `OutputRuntime` owns a 16-bit `channel_data: Vec<u16>` buffer and projects
  high bytes into `OutputState.channel_data` for client sync.

M4 will need adapter/runtime nodes to publish and consume equivalent products on
the core engine stack. M3.2 should define enough storage rules that M4 does not
invent them inside adapters.

## Prior Art

TouchDesigner separates data domains by operator family. Image/texture-like TOPs
are domain-specific products, often GPU-resident, and are cooked/pulled by
downstream consumers. CHOP-style channel/scalar data is a separate domain. The
useful lesson for M3.2 is that texture/image payloads should not be treated as
ordinary scalar values.

LX Studio is closer to the fixture/output path. It uses model-sized color
buffers for patterns/layers, and output classes convert those color buffers into
raw protocol byte buffers with byte ordering, gamma, brightness, and throttling
policy. The useful lesson for M3.2 is the separation between logical color
buffer, raw output byte buffer, and output send policy.

M3.2 should borrow the pattern, not clone either system:

- `RuntimeProduct::Value`: scalar/shader-compatible values;
- `RuntimeProduct::Render`: sampleable visual product handle;
- `RuntimeProduct::Buffer`: raw/color/texture buffer handle;
- `RuntimeBufferStore`: authoritative bytes plus metadata/version;
- `SyncProjection`: compatibility full copies into legacy `NodeState`.

## Scalar / Legacy Bridges

Some engine APIs still intentionally use `LpsValueF32`:

- `RuntimePropAccess`
- `ResolvedSlot`
- `Bus`
- model/wire bridge conversion helpers

These should remain scalar/legacy bridges for slot resolution, sync/tooling
reflection, and shader-compatible values. They are not the product-domain path
for texture-like runtime data.

# Questions

## Q1: Should M3.2 extend `RenderProductStore` or introduce a sibling buffer store?

Context: `RenderProductStore` already exists on `Engine` and owns sampleable
products. Texture-like products are render/sampling-compatible, but fixture lamp
colors and output channel bytes are raw/color buffers and may not be naturally
sampleable.

Answer: Introduce a sibling `RuntimeBufferStore` for raw/color/texture byte
storage, and keep `RenderProductStore` focused on sampleable products. Add
bridge points later where a texture buffer can also be exposed as a render
product.

## Q2: What should the first buffer handle be called?

Context: We need an opaque, copyable handle similar to `RenderProductId`.
Candidate names include `RuntimeBufferId`, `BufferProductId`, `RuntimeBlobId`,
or domain-specific IDs like `TextureBufferId`.

Answer: Use `RuntimeBufferId` for the generic store handle. Use metadata/kind to
distinguish texture/color/output payloads rather than creating separate ID types
in M3.2.

## Q3: What payload domains should the first store represent?

Context: M3.1's handoff has texture bytes, fixture lamp color bytes, output
channel bytes, and mapping cells. Mapping cells are structured geometry-ish
metadata, not obviously a raw byte buffer.

Answer: Represent byte-heavy payloads first: texture bytes, fixture lamp colors,
and output channel bytes. Keep mapping cells in projected `FixtureState` for now
unless M4 finds they need store-backed identity.

## Q4: Should buffers carry metadata and version together?

Context: Legacy wire state tracks bytes and dimensions/format with separate
`Versioned` fields. Runtime stores likely need a single changed frame for the
payload/metadata snapshot, plus metadata for sync projection and sampling.

Answer: Store each buffer entry as a versioned buffer payload rather than a
custom `changed_frame` field. The payload should include `{ kind, metadata,
bytes }` and be wrapped in the existing `Versioned<T>` pattern so the version
vocabulary stays consistent with the rest of the codebase. For texture buffers,
metadata includes width, height, format, and byte length. For color/output
buffers, metadata includes layout/element format enough for projection.

## Q5: How should `RuntimeProduct` refer to buffers?

Context: Today `RuntimeProduct` can be `Value(LpsValueF32)` or
`Render(RenderProductId)`. If M3.2 adds runtime buffers, product-domain
resolution may need to return a buffer handle directly rather than only a
sampleable render handle.

Answer: Add `RuntimeProduct::Buffer(RuntimeBufferId)` because M3.2 creates a
sibling buffer store. Keep `Render(RenderProductId)` for sampleable products.
Add a guard/helper that treats `LpsValueF32::Texture2D` as invalid for
`RuntimeProduct::value` or provides a checked constructor.

## Q6: Should M3.2 forbid `RuntimeProduct::Value(Texture2D)` at compile time or runtime?

Context: `LpsValueF32` includes `Texture2D` for shader ABI. Rust cannot express
"all `LpsValueF32` except `Texture2D`" without a new scalar type. A new type
could be cleaner but may expand scope.

Answer: Keep the enum shape but add a checked constructor such as
`RuntimeProduct::try_value` or `RuntimeProduct::scalar_value` returning an error
for `Texture2D`. Leave the existing `value` constructor only if all call sites
are audited and documented, or rename it if the churn is acceptable.

## Q7: What should the wire layer see in M3.2?

Context: M3.1 kept legacy `NodeState` heavy fields as compatibility snapshots.
The user explicitly wants future possibilities like slower texture updates,
compression, scaling, and diffing, but not a deep transport project now.

Answer: In M3.2, "snapshot" means a full copy of the current buffer payload
projected into existing legacy `NodeState` fields. Do not add wire
refs/diffs/chunks yet, but design the store so M3.2 or later can compare buffer
versions and later project either full snapshots or diffs.

## Q8: Should stores live on `Engine` now?

Context: `Engine` already owns `RenderProductStore`. M4 adapters will need a
central owner for runtime buffers to avoid hidden per-node ownership rules.

Answer: Yes. Add the buffer store to `Engine` beside `RenderProductStore`, with
accessors. Do not yet thread it through every node context unless a phase
explicitly needs a minimal adapter/test hook.

## Q9: What validation is enough for M3.2?

Context: This is foundation work in `lpc-engine`, with docs and tests. It should
not require firmware validation unless it touches shader compile/execute paths.

Answer: Use focused host validation: `cargo test -p lpc-engine` or targeted
`lpc-engine` module tests. If `lpc-wire` types are touched, add
`cargo test -p lpc-wire`.

# Notes

- Do not remove or gate the embedded shader compiler.
- Do not remove `LpsValueF32::Texture2D`; it remains shader ABI.
- Prefer minimal store identity now over transport policy.
