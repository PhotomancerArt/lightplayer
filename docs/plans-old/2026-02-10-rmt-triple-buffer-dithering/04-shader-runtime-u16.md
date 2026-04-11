# Phase 4: Shader Runtime u16 Output

## Goal

Shader runtime writes u16 to texture instead of u8. Texture is Rgba16 (from phase 2).

## Tasks

### 4.1 Shader output conversion

Currently: Q32 → u8 via `(q32 * 255) / 65536`

Change to: Q32 → u16 via `(q32 * 65535) / 65536` or equivalent (Q32 is i32 in [0, 65536) for 0..1).

Ensure clamping: Q32 values must map to [0, 65535].

### 4.2 Texture write path

Shader runtime calls texture write. With Rgba16 texture:
- Use `set_pixel_u16` or equivalent
- Pass `[r, g, b, a]` as u16

### 4.3 Texture runtime

From phase 2: texture runtime hardcodes Rgba16. So texture created with Rgba16.
Shader runtime must use the u16 write API.

## Verification

- Shader renders to Rgba16 texture
- Fixture (phase 5) can sample and see 16-bit values
