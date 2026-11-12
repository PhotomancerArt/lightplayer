# Phase 2: TextureFormat Rgba16

## Goal

Add Rgba16 format to lp-model and support it in lp-shared Texture. No engine/fixture changes yet.

## Tasks

### 2.1 lp-model TextureFormat

In `lp-model/src/nodes/texture/format.rs`:

- Add `Rgba16` variant
- `bytes_per_pixel()`: 8 for Rgba16
- `as_str()`: "RGBA16"
- `from_str()`: "RGBA16" => Some(Rgba16)
- Update `Default` if needed (keep Rgba8 for backward compat during migration, or switch – check usages)

### 2.2 lp-shared Texture

In `lp-shared/src/util/texture.rs`:

- `get_pixel()`: for Rgba16, read 8 bytes, return `[u8; 4]` (high byte of each u16, or scaled – see 00-design for "unsigned normalized u16")
- `set_pixel()`: add `set_pixel_u16(&mut self, x, y, color: [u16; 4])` for Rgba16, or overload based on format

For Rgba16, pixel layout: R low, R high, G low, G high, B low, B high, A low, A high (little-endian u16).

### 2.3 Texture::sample for Rgba16

Ensure `sample()` handles Rgba16 – may need format-specific logic. For now, `get_pixel` at sampled coords may suffice if bilinear is format-agnostic.

## Verification

- Texture::new with Rgba16 allocates correct size
- set_pixel_u16 / get_pixel round-trip
