# Phase 7: Implement abstraction traits

## Goal

Create platform-agnostic traits for filesystem, transport, and LED output.

## Tasks

1. Create `src/traits/filesystem.rs`:
   - `Filesystem` trait with methods:
     - `read_file(path: &str) -> Result<Vec<u8>, Error>`
     - `write_file(path: &str, data: &[u8]) -> Result<(), Error>`
     - `file_exists(path: &str) -> Result<bool, Error>`
2. Create `src/traits/transport.rs`:
   - `Transport` trait with methods:
     - `send_message(message: &str) -> Result<(), Error>`
     - `receive_message() -> Result<String, Error>` (blocking, reads until `\n`)
   - Note: Abstracted at JSON string level, not serial port level
3. Create `src/traits/led_output.rs`:
   - `LedOutput` trait with methods:
     - `write_pixels(pixels: &[u8]) -> Result<(), Error>`
     - `get_pixel_count() -> usize`
   - `Pixel` type alias or struct for RGB/RGBA
4. Export all traits from `src/traits/mod.rs`

## Success Criteria

- All traits are defined and exported
- Traits are platform-agnostic (no std or hardware-specific types)
- Traits compile without warnings

