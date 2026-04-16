# Phase 1: Add lps-builtins dependency and Q32 imports

## Scope of phase

Add `lps-builtins` as a dependency to `lp-engine` and import Q32 type for fixed-point math
operations.

## Code Organization Reminders

- Place dependency changes in `Cargo.toml`
- Import statements should be organized at the top of modules
- Keep imports minimal - only import what we need

## Implementation Details

### 1. Update Cargo.toml

Add `lps-builtins` dependency to `lp-app/crates/lp-engine/Cargo.toml`:

```toml
[dependencies]
# ... existing dependencies ...
lps-builtins = { path = "../../../lp-shader/lps-builtins", default-features = false }
```

Note: We'll use `default-features = false` to keep it minimal for embedded use.

### 2. Verify Q32 is accessible

Check that we can import Q32 from `lps-builtins`:

```rust
use lps_builtins::glsl::q32::types::Q32;
```

We'll add this import in the next phase when we create `mapping_compute.rs`.

## Validate

Run:

```bash
cd lp-app && cargo check --package lp-engine
```

Expected: Compiles successfully with new dependency.
