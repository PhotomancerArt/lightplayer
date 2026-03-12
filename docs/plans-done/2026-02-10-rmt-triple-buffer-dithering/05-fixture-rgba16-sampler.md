# Phase 5: Fixture Rgba16Sampler + u16 Output

## Goal

Fixture samples Rgba16 texture and outputs u16 to the output buffer. Add Rgba16Sampler, extend ColorOrder for u16, change output buffer interface.

## Tasks

### 5.1 Rgba16Sampler

In `lp-engine/.../fixture/mapping/sampling/`:

- Create `rgba16.rs` with `Rgba16Sampler`
- `TextureSampler::sample_pixel`: for Rgba16, return `Option<[u16; 3]>` or adapt trait
- Current trait returns `Option<[u8; 3]>` – we need u16 for 16-bit pipeline. Either:
  - Extend trait with `sample_pixel_u16` and implement for Rgba16 (Rgba8 returns scaled), or
  - Change trait to return a generic/int that fixture converts – more invasive
  - Simplest: add `sample_pixel_u16` to trait, default impl for Rgba8: `sample_pixel().map(|[r,g,b]| [r as u16 * 257, g as u16 * 257, b as u16 * 257])`, Rgba16 reads u16 directly

### 5.2 create_sampler

- Add `TextureFormat::Rgba16 => Box::new(rgba16::Rgba16Sampler)`

### 5.3 Accumulation

- `accumulate_from_mapping` and `ChannelAccumulators`: currently Q32. For 16-bit output we need fixture to write u16 to output.
- Fixture render: instead of `to_u8_clamped()` and `ColorOrder::write_rgb(buffer, r, g, b: u8)`, use u16 path.
- `ColorOrder`: add `write_rgb_u16(buffer, offset, r, g, b: u16)` or equivalent.
- Output buffer type: `get_output` returns `&mut [u16]` with ch_count = 3 (meaning 3×u16 per pixel for RGB).

### 5.4 Fixture → Output handoff

- Output's `get_buffer_mut(start_ch, ch_count)` currently returns `&mut [u8]` with ch_count=3.
- Change to return `&mut [u16]` when in 16-bit mode, or change output to always use 16-bit.
- Fixture asks for ch_count=3 (RGB) – in 16-bit that’s 3 u16s per pixel. Layout: [r0,g0,b0, r1,g1,b1, ...].

## Verification

- Fixture samples Rgba16 texture, accumulates, writes u16 to output buffer
- Output can read 16-bit data
