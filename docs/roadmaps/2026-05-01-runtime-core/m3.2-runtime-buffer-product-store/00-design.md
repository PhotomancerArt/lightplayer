# M3.2: Runtime Buffer Product Store Design

## Scope of Work

M3.2 adds the minimal runtime-owned buffer/product foundation needed before M4
ports legacy shader -> fixture -> output behavior onto the core engine.

The goal is to make raw/texture/color byte ownership explicit. Texture pixels,
fixture lamp colors, and output channel bytes should not be hidden inside scalar
values or treated as authoritative legacy wire state. The wire layer can still
receive full compatibility copies through `SyncProjection`; runtime ownership
moves to engine stores.

In scope:

- Add a generic runtime buffer store with opaque IDs.
- Store buffer entries as `Versioned<RuntimeBuffer>` using existing version
  vocabulary.
- Add runtime buffer metadata for texture, fixture color, output channel, and
  raw byte payloads.
- Add `RuntimeProduct::Buffer(RuntimeBufferId)` for product-domain references.
- Add a checked scalar-value constructor that rejects
  `LpsValueF32::Texture2D`.
- Add `RuntimeBufferStore` ownership to `Engine`.

Out of scope:

- Wire refs, binary chunks, compression, scaling, throttling, or diff algorithms.
- Replacing legacy `GetChanges` / `NodeState` snapshots.
- Porting legacy runtime nodes onto core `Engine`.
- GPU-backed resources, cross-engine sharing, eviction policy, or async
  production.
- Removing `LpsValueF32::Texture2D` from shader ABI paths.

## File Structure

```text
docs/roadmaps/2026-05-01-runtime-core/
└── m3.2-runtime-buffer-product-store/
    ├── notes.md
    ├── 00-notes.md
    ├── 00-design.md
    ├── 01-add-runtime-buffer-store.md
    ├── 02-wire-buffer-store-into-engine.md
    ├── 03-extend-runtime-product-buffer-domain.md
    ├── 04-cleanup-validation-summary.md
    └── summary.md

lp-core/lpc-engine/src/
├── runtime_buffer/
│   ├── mod.rs
│   ├── runtime_buffer_id.rs
│   ├── runtime_buffer.rs
│   └── runtime_buffer_store.rs
├── runtime_product/
│   └── runtime_product.rs
├── engine/
│   └── engine.rs
└── lib.rs
```

## Conceptual Architecture

```text
Engine
  ├─ Resolver
  │   └─ Production
  │       └─ Versioned<RuntimeProduct>
  │           ├─ Value(LpsValueF32)       # scalar/shader-compatible, not Texture2D
  │           ├─ Render(RenderProductId)  # sampleable visual product handle
  │           └─ Buffer(RuntimeBufferId)  # raw/texture/color buffer handle
  │
  ├─ RenderProductStore
  │   └─ RenderProductId -> sampleable RenderProduct
  │
  └─ RuntimeBufferStore
      └─ RuntimeBufferId -> Versioned<RuntimeBuffer>
          ├─ kind: Texture | FixtureColors | OutputChannels | Raw
          ├─ metadata: dimensions/layout/format
          └─ bytes: Vec<u8>

SyncProjection / legacy GetChanges
  └─ copies current Versioned<RuntimeBuffer> into legacy NodeState fields
     when compatibility wire snapshots are needed
```

## Main Components

### RuntimeBufferStore

`RuntimeBufferStore` is a sibling to `RenderProductStore`, not an extension of
it. It owns byte-heavy runtime payloads that may not be sampleable:

- texture bytes;
- fixture lamp color bytes;
- output channel bytes;
- future raw byte payloads.

Entries are stored as `Versioned<RuntimeBuffer>`, where `RuntimeBuffer` contains
the payload kind, metadata, and bytes. Using `Versioned<T>` keeps buffer version
language aligned with the rest of the runtime.

### RuntimeBufferId

`RuntimeBufferId` is a small, copyable, opaque handle like `RenderProductId`.
M3.2 uses one generic ID type and metadata/kind to distinguish buffer domains.
Separate domain-specific IDs can be introduced later if type safety becomes
worth the extra surface.

### RuntimeProduct::Buffer

`RuntimeProduct` gains a `Buffer(RuntimeBufferId)` variant so product-domain
resolution can return non-sampleable buffers directly. `Render(RenderProductId)`
continues to mean sampleable visual product.

`RuntimeProduct::Value` remains for scalar/shader-compatible values, but M3.2
adds a checked constructor that rejects `LpsValueF32::Texture2D`. `Texture2D`
stays valid in shader ABI code, but core product-domain texture-like payloads
should use handles.

### SyncProjection Compatibility

M3.2 does not change legacy wire transport. A "snapshot" means a full copy of
the current buffer payload projected into existing legacy `NodeState` fields.
Later milestones can project refs, diffs, compressed chunks, or throttled
updates from the same store identity.

## Phase Outline

1. Add Runtime Buffer Store                         [sub-agent: yes,        model: kimi-k2.5, parallel: -]
2. Wire Buffer Store Into Engine                    [sub-agent: yes,        model: composer-2, parallel: 3 after 1]
3. Extend RuntimeProduct Buffer Domain              [sub-agent: yes,        model: kimi-k2.5, parallel: 2 after 1]
4. Cleanup, review, and validation                  [sub-agent: supervised, model: gpt-5.5,   parallel: -]
