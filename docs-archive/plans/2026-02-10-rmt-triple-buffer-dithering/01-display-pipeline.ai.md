# Phase 1: DisplayPipeline in lp-shared

## Goal

Implement the DisplayPipeline module in `lp-shared` with options, LUT, dithering, and tick logic. No fw deps, unit-testable.

## Tasks

### 1.1 Module structure

Create `lp-shared/src/display_pipeline/`:

```
display_pipeline/
  mod.rs      # re-exports, pub API
  options.rs  # DisplayPipelineOptions + Default
  lut.rs      # build LUT, apply (lut_interpolate)
  dither.rs   # dither step (overflow + round)
  pipeline.rs # DisplayPipeline struct, write_frame, tick
```

Add `pub mod display_pipeline` to `lp-shared/src/lib.rs`.

### 1.2 DisplayPipelineOptions

```rust
pub struct DisplayPipelineOptions {
    pub lum_power: f32,
    pub white_point: [f32; 3],
    pub interpolation_enabled: bool,
    pub dithering_enabled: bool,
    pub lut_enabled: bool,
}
```

`Default`: lum_power=2, white_point=[0.9, 1.0, 1.0], interpolation=true, dithering=true, lut=true.

### 1.3 LUT (lut.rs)

- 257 entries per channel (u32). Build from lum_power + white_point.
- Formula: `lut[i] = clamp(round(pow((i/256) * white_point[c], lum_power) * 0xFFFF), 0, 0xFFFF)`
- Interpolate: `(lut[index] * (0x100 - alpha) + lut[index+1] * alpha) >> 8` where index = value>>8, alpha = value&0xFF

### 1.4 Dither (dither.rs)

- Per-pixel overflow `[i8; 3]`
- Output: `((interpolated + overflow + 0x80) >> 8).clamp(0, 255)`
- New overflow: `interpolated + overflow - (output as i32 * 257)`

### 1.5 DisplayPipeline (pipeline.rs)

- `prev`, `current`, `next`: `Vec<u16>` each `num_leds * 3`
- `prev_ts`, `current_ts`, `next_ts`: u64
- `has_prev`, `has_current`, `has_next`: bool
- `dither_overflow`: `Vec<[i8; 3]>`
- `lut`: `[[u32; 257]; 3]`

Methods:
- `new(num_leds, options) -> Result<Self, Error>` – allocate, build LUT
- `write_frame(&mut self, ts_us: u64, data: &[u16])` – rotate, copy to next, set has_next
- `write_frame_from_u8(&mut self, ts_us: u64, data: &[u8])` – expand u8→u16, call write_frame
- `tick(&mut self, now_us: u64, out: &mut [u8])` – per Q4 logic, no alloc

### 1.6 Unit tests

- LUT: verify entries for known inputs
- Dither: round-trip with overflow
- Pipeline: write_frame → tick produces output; interpolation blend; hold when !has_next

## Dependencies

- `alloc` only
- No `lp_model`, `lp_engine`, or fw crates
