# Phase 3: OutputProvider Trait Changes

## Goal

Add optional `options` to `open()` and change `write()` to accept 16-bit data. Update MemoryOutputProvider.

## Tasks

### 3.1 OutputDriverOptions

In `lp-shared` (e.g. `output/options.rs` or `display_pipeline/options.rs`):

- `OutputDriverOptions` = type alias or re-export of `DisplayPipelineOptions`

### 3.2 OutputProvider::open

```rust
fn open(
    &self,
    pin: u32,
    byte_count: u32,
    format: OutputFormat,
    options: Option<OutputDriverOptions>,
) -> Result<OutputChannelHandle, OutputError>;
```

- `byte_count`: number of bytes for the 8-bit output (num_leds * 3). Provider derives num_leds = byte_count / 3 for DisplayPipeline.
- `options`: passed through; MemoryOutputProvider ignores it.

### 3.3 OutputProvider::write

```rust
fn write(&self, handle: OutputChannelHandle, data: &[u16]) -> Result<(), OutputError>;
```

- `data`: 16-bit RGB, layout `[r,g,b; num_leds]`. Length = num_leds * 3.
- Update `OutputError` if needed (e.g. `DataLengthMismatch` for u16 count).

### 3.4 MemoryOutputProvider

- `ChannelState`: change `data: Vec<u8>` to `data: Vec<u16>` (or keep u8 and convert – for tests we may want to store 16-bit to verify)
- `open()`: accept `options`, ignore
- `write()`: accept `&[u16]`, store or compare
- `get_data()`: return `Vec<u16>` or provide both u8/u16 accessors for tests

### 3.5 Call sites

- All `ctx.output_provider().open(...)` need 4th arg `None` initially
- All `ctx.output_provider().write(handle, ...)` – will fail until Output runtime passes u16
- Do this in phase 6 when Output runtime switches to 16-bit; for now trait change may break callers – add `None` and a temporary adapter if needed

**Note**: Phases 3 and 6 may need to be coordinated so write() callers are updated when trait changes. Alternative: add `write_u16` as new method, deprecate `write`, migrate callers, then remove old `write`. Simpler: do trait change and fix all callers in same phase or adjacent phases.
