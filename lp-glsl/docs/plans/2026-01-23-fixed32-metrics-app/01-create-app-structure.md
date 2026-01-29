# Phase 1: Create App Structure and Dependencies

## Description

Create the basic app structure with `Cargo.toml` and directory layout. Set up dependencies on `lp-glsl-compiler`, `serde`, `toml`, and `clap`.

## Implementation

- Create `lp-glsl/apps/q32-metrics/` directory
- Create `Cargo.toml` with:
  - Package name: `q32-metrics`
  - Dependencies: `lp-glsl-compiler` (with `std` feature), `serde`, `toml`, `clap`
- Create `src/` directory
- Create `glsl/` directory for test files
- Create `README.md` with basic app description

## Success Criteria

- `Cargo.toml` exists with correct dependencies
- Directory structure is in place
- `cargo check` succeeds
