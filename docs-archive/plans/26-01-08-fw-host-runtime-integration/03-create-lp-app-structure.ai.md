# Phase 3: Create LpApp structure and constructor

## Goal

Create the `LpApp` struct and basic constructor.

## Tasks

1. Create `lp-core/src/app/lp_app.rs`:
   - `LpApp` struct:
     - `platform: Platform`
     - `runtime: Option<ProjectRuntime>`
   - Constructor: `new(platform: Platform) -> Self`
     - Initializes with `None` runtime
     - Stores platform

2. Update `lp-core/src/app/mod.rs` to export `lp_app::LpApp`

3. Add basic documentation comments

## Success Criteria

- `LpApp` struct compiles
- Can be constructed with `Platform`
- Exported from `lp-core` crate
- Code compiles without warnings

