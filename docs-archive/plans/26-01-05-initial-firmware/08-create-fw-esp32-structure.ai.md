# Phase 8: Create fw-esp32 app structure and basic setup

## Goal

Create the ESP32 firmware application structure with basic ESP32-C6 setup.

## Tasks

1. Create `lightplayer/apps/fw-esp32/` directory
2. Create `Cargo.toml` with:
   - Dependencies: `lp-core`, `esp-hal`, `esp-hal-embassy`, `esp-alloc`, etc.
   - Similar to `esp32-glsl-jit` but for firmware
3. Create `src/main.rs` with:
   - Basic ESP32-C6 setup (similar to `esp32-glsl-jit`)
   - Heap allocator setup
   - Embassy executor setup
   - RTT/debug output setup
4. Create `build.rs` for linker scripts
5. Add `fw-esp32` to `lightplayer/Cargo.toml` workspace members
6. Create basic project structure (will be filled in later phases)

## Success Criteria

- `fw-esp32` app exists and compiles
- Basic ESP32-C6 initialization works
- App is added to workspace
- Code compiles without warnings

