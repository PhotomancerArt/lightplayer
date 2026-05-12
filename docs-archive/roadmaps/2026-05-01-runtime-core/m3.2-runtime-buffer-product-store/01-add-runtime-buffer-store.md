# Phase 1: Add Runtime Buffer Store

## Scope of Phase

Add the `lpc-engine` runtime buffer module and store. This phase establishes the
types and tests, but does not wire the store into `Engine` and does not modify
`RuntimeProduct`.

In scope:

- Add `lp-core/lpc-engine/src/runtime_buffer/`.
- Add `RuntimeBufferId`.
- Add `RuntimeBuffer`, `RuntimeBufferKind`, and metadata types.
- Add `RuntimeBufferStore` that owns `Versioned<RuntimeBuffer>` entries.
- Export the module from `lpc-engine` so later phases can use it.
- Add focused unit tests.

Out of scope:

- `Engine` ownership/accessors; that is Phase 2.
- `RuntimeProduct::Buffer`; that is Phase 3.
- Wire protocol changes.
- Any legacy runtime porting.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public types / entry points first, support code next, helpers near the
  bottom, tests at the end.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If blocked by a design choice, stop and report rather than improvising.
- Report back: files changed, validation run, result, and any deviations.

## Implementation Details

Create these files:

- `lp-core/lpc-engine/src/runtime_buffer/mod.rs`
- `lp-core/lpc-engine/src/runtime_buffer/runtime_buffer_id.rs`
- `lp-core/lpc-engine/src/runtime_buffer/runtime_buffer.rs`
- `lp-core/lpc-engine/src/runtime_buffer/runtime_buffer_store.rs`

Update:

- `lp-core/lpc-engine/src/lib.rs`

Suggested type shape:

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeBufferId(u32);

impl RuntimeBufferId {
    #[must_use]
    pub const fn new(raw: u32) -> Self { Self(raw) }

    #[must_use]
    pub const fn as_u32(self) -> u32 { self.0 }
}
```

`RuntimeBuffer` should be cloneable/debuggable and own bytes:

```rust
pub struct RuntimeBuffer {
    pub kind: RuntimeBufferKind,
    pub metadata: RuntimeBufferMetadata,
    pub bytes: Vec<u8>,
}
```

Use metadata/kind to cover the first domains:

```rust
pub enum RuntimeBufferKind {
    Texture,
    FixtureColors,
    OutputChannels,
    Raw,
}

pub enum RuntimeBufferMetadata {
    Texture {
        width: u32,
        height: u32,
        format: RuntimeTextureFormat,
    },
    FixtureColors {
        channels: u32,
        layout: RuntimeColorLayout,
    },
    OutputChannels {
        channels: u32,
        sample_format: RuntimeChannelSampleFormat,
    },
    Raw,
}
```

Keep these metadata enums in `runtime_buffer.rs` unless the file gets too large:

- `RuntimeTextureFormat`
  - `Rgba16`
  - `Rgb8`
- `RuntimeColorLayout`
  - `Rgb8`
- `RuntimeChannelSampleFormat`
  - `U8`
  - `U16`

Add helper constructors if they keep tests readable:

- `RuntimeBuffer::texture_rgba16(width, height, bytes)`
- `RuntimeBuffer::fixture_colors_rgb8(channels, bytes)`
- `RuntimeBuffer::output_channels_u8(channels, bytes)`
- `RuntimeBuffer::raw(bytes)`

`RuntimeBufferStore` should mirror the simple style of `RenderProductStore`:

```rust
pub struct RuntimeBufferStore {
    next_id: u32,
    buffers: BTreeMap<RuntimeBufferId, Versioned<RuntimeBuffer>>,
}

impl RuntimeBufferStore {
    pub fn new() -> Self;
    pub fn insert(&mut self, buffer: Versioned<RuntimeBuffer>) -> RuntimeBufferId;
    pub fn get(&self, id: RuntimeBufferId) -> Option<&Versioned<RuntimeBuffer>>;
    pub fn get_mut(&mut self, id: RuntimeBufferId) -> Option<&mut Versioned<RuntimeBuffer>>;
    pub fn replace(
        &mut self,
        id: RuntimeBufferId,
        buffer: Versioned<RuntimeBuffer>,
    ) -> Result<(), RuntimeBufferError>;
}
```

Add `RuntimeBufferError::UnknownBuffer { id: RuntimeBufferId }`.

Export from `runtime_buffer/mod.rs`:

- `RuntimeBufferId`
- `RuntimeBuffer`
- `RuntimeBufferKind`
- `RuntimeBufferMetadata`
- metadata enums
- `RuntimeBufferStore`
- `RuntimeBufferError`

Update `lp-core/lpc-engine/src/lib.rs`:

- add `pub mod runtime_buffer;`
- re-export the public runtime buffer types near the other engine exports.

Tests to include:

- `RuntimeBufferId` round-trips raw ID.
- Store inserts and retrieves a versioned texture buffer with metadata and bytes.
- Store replaces an existing buffer and preserves the new `Versioned` frame.
- Store returns `UnknownBuffer` when replacing a missing ID.
- Fixture/output helper constructors set kind and metadata correctly.

## Validate

Run:

```bash
cargo test -p lpc-engine runtime_buffer
```
