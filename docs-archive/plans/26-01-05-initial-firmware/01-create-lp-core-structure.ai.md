# Phase 1: Create lp-core crate structure and basic types

## Goal

Create the `lp-core` crate with the basic directory structure and foundational types.

## Tasks

1. Create `lightplayer/crates/lp-core/` directory
2. Create `Cargo.toml` with:
   - `no_std` with `alloc` feature
   - Dependencies: `serde`, `serde_json` (with `alloc` feature, no default features)
   - Edition 2024
3. Create directory structure:
   ```
   src/
   ├── lib.rs
   ├── project/
   │   └── mod.rs
   ├── nodes/
   │   └── mod.rs
   ├── protocol/
   │   └── mod.rs
   ├── error.rs
   ├── traits/
   │   └── mod.rs
   └── util/
       └── mod.rs
   ```
4. Set up module exports in `lib.rs`
5. Add `lp-core` to `lightplayer/Cargo.toml` workspace members

## Success Criteria

- `lp-core` crate exists and compiles
- All modules are properly set up and exported
- Crate is added to workspace
- Code compiles without warnings (except unused code that will be used later)

