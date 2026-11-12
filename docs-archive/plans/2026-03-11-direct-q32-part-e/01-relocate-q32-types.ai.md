# Phase 1: Relocate Shared Q32 Types

## Goal

Move `Q32Options`, `float_to_fixed16x16`, and `FixedPointFormat` out of
`backend/transform/q32/` into a new `backend/q32/` module, so the transform
directory can be deleted cleanly.

## New module structure

```
backend/q32/
  mod.rs       — pub use re-exports
  options.rs   — Q32Options + Q32OptionsBuilder (moved from transform/q32/options.rs)
  types.rs     — FixedPointFormat, float_to_fixed16x16, etc. (moved from transform/q32/types.rs)
```

## Steps

1. Create `backend/q32/mod.rs`:

```rust
pub mod options;
pub mod types;

pub use options::Q32Options;
pub use types::{FixedPointFormat, float_to_fixed16x16};
```

2. Copy `backend/transform/q32/options.rs` → `backend/q32/options.rs` (unchanged)

3. Copy `backend/transform/q32/types.rs` → `backend/q32/types.rs` (unchanged)

4. Add `pub mod q32;` to `backend/mod.rs`

5. Update imports:
   - `numeric.rs` line 7: `use crate::backend::q32::{Q32Options, float_to_fixed16x16};`
   - `lib.rs` line 22: `pub use backend::q32::Q32Options;`
   - `exec/executable.rs`: update any `Q32Options` import
   - `exec/jit.rs`: update any `Q32Options` / `float_to_fixed16x16` import

6. Verify: `cargo check` — all production code compiles with new paths.
   Transform code will still compile at this point (it uses its own local paths).
