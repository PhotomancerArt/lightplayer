# Phase 9: Implement ESP32 filesystem integration

## Goal

Implement filesystem access on ESP32 using esp-storage + littlefs2.

## Tasks

1. Add dependencies to `fw-esp32/Cargo.toml`:
   - `esp-storage`
   - `littlefs2`
2. Create `src/filesystem.rs`:
   - Implement `littlefs2` block device trait using `esp-storage`
   - Create adapter that wraps esp-hal Flash with littlefs2 block device interface
   - Implement `lp-core::traits::Filesystem` trait for ESP32
3. Initialize filesystem in `main.rs`:
   - Set up partition (1MB at 0x300000)
   - Mount littlefs2 filesystem
   - Create filesystem instance
4. Test basic file read/write operations

## Success Criteria

- Filesystem can be mounted and accessed
- `Filesystem` trait implementation works
- Can read/write files on ESP32
- All code compiles without warnings

