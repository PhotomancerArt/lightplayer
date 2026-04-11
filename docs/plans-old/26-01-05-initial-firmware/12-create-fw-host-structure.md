# Phase 12: Create fw-host app structure and basic setup

## Goal

Create the host firmware application structure.

## Tasks

1. Create `lightplayer/apps/fw-host/` directory
2. Create `Cargo.toml` with:
   - Dependencies: `lp-core`, `egui`, `std` features
   - Standard library enabled
3. Create `src/main.rs` with:
   - Basic application setup
   - egui window initialization
   - Main event loop
4. Add `fw-host` to `lightplayer/Cargo.toml` workspace members
5. Create basic project structure (will be filled in later phases)

## Success Criteria

- `fw-host` app exists and compiles
- Basic egui window opens
- App is added to workspace
- Code compiles without warnings

