# Phase 1: Create Platform struct

## Goal

Create the `Platform` struct that wraps platform-specific trait implementations.

## Tasks

1. Create `lp-core/src/app/platform.rs`:
   - `Platform` struct with:
     - `fs: Box<dyn Filesystem>`
     - `output: Box<dyn OutputProvider>`
   - Constructor: `new(fs: Box<dyn Filesystem>, output: Box<dyn OutputProvider>) -> Self`

2. Create `lp-core/src/app/mod.rs`:
   - Export `platform::Platform`
   - Export `lp_app::LpApp` (will be added in phase 3)
   - Export `messages::{MsgIn, MsgOut}` (will be added in phase 2)

3. Update `lp-core/src/lib.rs` to include `app` module

## Success Criteria

- `Platform` struct compiles
- Can be constructed with trait objects
- Exported from `lp-core` crate
- Code compiles without warnings

