# Phase 2: Texture Utility

## Goal

Implement the low-level Texture abstraction for managing pixel buffers.

## Tasks

1. Create `util/texture.rs` with:
   - `Texture` struct: `width: u32`, `height: u32`, `format: String`, `data: Vec<u8>`
   - `Texture::new(width, height, format)` constructor:
     - Validates format string (RGB8, RGBA8, R8)
     - Allocates buffer: `Vec::with_capacity(width * height * bytes_per_pixel(format))`
     - Initializes buffer to zeros
     - Returns `Result<Texture, Error>` on invalid format
   - `format() -> &str` method
   - `bytes_per_pixel() -> usize` method (derives from format string)
   - `get_pixel(x: u32, y: u32) -> Option<[u8; 4]>` method
   - `set_pixel(x: u32, y: u32, color: [u8; 4])` method (writes based on format: RGB8=first 3 bytes, R8=first byte, RGBA8=all 4 bytes)
   - `sample(u: f32, v: f32) -> Option<[u8; 4]>` method (normalized coordinates, bilinear sampling)
   - `compute_all<F>(f: F)` method where `F: Fn(u32, u32) -> [u8; 4]`
   - Helper function `bytes_per_pixel(format: &str) -> Option<usize>` for format validation

2. Update `util/mod.rs` to export texture module

3. Add tests:
   - Test Texture::new() with valid formats
   - Test Texture::new() with invalid format (returns error)
   - Test get_pixel/set_pixel for RGB8, RGBA8, R8 formats
   - Test sample() with normalized coordinates
   - Test compute_all() helper
   - Test buffer initialization (all zeros)

## Success Criteria

- Texture struct compiles and works correctly
- All format types (RGB8, RGBA8, R8) work correctly
- Pixel operations (get/set) work correctly for each format
- Sampling works correctly
- All tests pass
- No warnings (except unused code that will be used in later phases)
- Code follows existing style

