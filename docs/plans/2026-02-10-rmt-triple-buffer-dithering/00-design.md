# RMT Triple-Buffer Dithering - Design

## Overview

Extend the ESP32 RMT driver with a triple-buffered display pipeline supporting optional interpolation and dithering. The pipeline lives in `lp-shared` (pure computation, no fw deps) and is integrated by `Esp32OutputProvider` when opening a channel.

**Data flow**: Shader → Texture (Rgba16) → Fixture → Output → DisplayPipeline → RMT (8-bit)

## DisplayPipeline

### Location

`lp-shared/src/display_pipeline/` – new module. Depends only on `alloc`, no `std` or fw deps. Unit-testable.

### API

```rust
/// Options for display pipeline (LUT, dithering, interpolation)
pub struct DisplayPipelineOptions {
    pub lum_power: f32,           // Gamma exponent (default 2)
    pub white_point: [f32; 3],    // RGB balance
    pub interpolation_enabled: bool,
    pub dithering_enabled: bool,
    pub lut_enabled: bool,
}

impl Default for DisplayPipelineOptions { ... }

/// Triple-buffered display pipeline. 16-bit in, 8-bit out.
pub struct DisplayPipeline { ... }

impl DisplayPipeline {
    /// Allocate pipeline. Returns Err if allocation fails.
    pub fn new(num_leds: u32, options: DisplayPipelineOptions) -> Result<Self, Error>;

    /// Submit 16-bit RGB frame for next buffer. Data layout: [r,g,b; num_leds] as u16.
    pub fn write_frame(&mut self, ts_us: u64, data: &[u16]);

    /// Submit 8-bit RGB frame (for porting). Expands to 16-bit internally.
    pub fn write_frame_from_u8(&mut self, ts_us: u64, data: &[u8]);

    /// Advance pipeline, produce 8-bit output. Call each display refresh.
    /// out: RGB8, num_leds * 3 bytes. ISR-safe (no alloc, no panic paths).
    pub fn tick(&mut self, now_us: u64, out: &mut [u8]);
}
```

### Internal structure

- **Frames**: 3 × `Vec<u16>` (prev, current, next), each `num_leds * 3`
- **Timestamps**: prev_ts, current_ts, next_ts (u64)
- **Flags**: has_prev, has_current, has_next
- **Dither overflow**: `Vec<[i8; 3]>` per pixel
- **LUT**: 3 × 257 entries (u32), built from lum_power + white_point at creation

### Tick logic (from Q4)

1. If interpolation enabled and `!has_prev`: use current only (no blend)
2. If `!has_current`: skip tick / output black or last frame
3. If `!has_next` and `now > current_ts`: hold current (freeze)
4. Otherwise: interpolate prev/current by progress, apply LUT, dither, write to `out`

### LUT (from LEDscape)

- 257 entries per channel (index 0..256)
- Input: 16-bit value; index = `value >> 8`, alpha = `value & 0xFF`
- Output: `(lut[i] * (0x100 - alpha) + lut[i+1] * alpha) >> 8`
- Build: `lut[i] = clamp(pow((i/256) * white_point[c], lum_power) * 0xFFFF)`

### Dithering (from LEDscape)

- Overflow: int8 per channel per pixel
- Output: `(interpolated + overflow + 0x80) >> 8` clamped to u8
- New overflow: `interpolated + overflow - (output * 257)`

## OutputProvider changes

### Trait (Q1)

Add optional `options` to `open()`:

```rust
fn open(
    &self,
    pin: u32,
    byte_count: u32,
    format: OutputFormat,
    options: Option<OutputDriverOptions>,  // NEW
) -> Result<OutputChannelHandle, OutputError>;
```

`OutputDriverOptions` = `DisplayPipelineOptions` (or alias). `MemoryOutputProvider` ignores `options`.

### Output node / config

- `OutputConfig::GpioStrip` extended with optional driver options (or new variant)
- When config changes: output node closes and reopens (Q3)

## Texture / Shader changes (Q2)

- Add `TextureFormat::Rgba16` to `lp-model`
- Add `bytes_per_pixel() = 8`, `as_str() = "RGBA16"`
- `Texture::set_pixel` overload or new method for `[u16; 4]`
- Texture runtime: hardcode `TextureFormat::Rgba16` (replace `Rgba8`)
- Shader runtime: write u16 to texture (Q32 → u16: `(q32 * 65535) / 65536` or similar)
- Add `Rgba16Sampler` for fixture sampling

## Fixture / Output data flow

- Fixture samples Rgba16 texture → accumulates Q32 (unchanged) → outputs u16 to output buffer
- Output buffer: `Vec<u16>` (6 bytes/px: R,G,B as u16)
- Output runtime: gets 16-bit data from fixtures, passes to provider via `write()`; provider runs DisplayPipeline, sends 8-bit to RMT

## Esp32OutputProvider integration

- On `open(pin, byte_count, format, options)`: create `DisplayPipeline::new(num_leds, options.unwrap_or_default())`
- Store pipeline per channel (with `ChannelState`)
- `OutputProvider::write(handle, data)`: change to accept 16-bit RGB. `byte_count` = `num_leds * 6`
- On write: call `pipeline.write_frame(ts_us, data)`, then `pipeline.tick(now_us, rmt_buffer)`, send 8-bit result to RMT
- RMT unchanged: receives 8-bit RGB

Full 16-bit path: engine output buffer is 16-bit, fixtures write u16, provider receives 16-bit and runs DisplayPipeline. `write_frame_from_u8` remains for testing/porting only.

## File layout

```
lp-shared/src/
  display_pipeline/
    mod.rs
    options.rs
    pipeline.rs
    lut.rs
    dither.rs
  lib.rs              # pub mod display_pipeline
```

## Migration (Q6)

- Update `demo_project.rs` and project templates to use Rgba16
- No migration machinery; project under active dev, not released
