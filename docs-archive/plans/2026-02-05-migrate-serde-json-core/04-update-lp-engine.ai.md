# Phase 4: Update lp-engine to Use Wrapper

## Scope of Phase

Replace all `serde_json` usage in `lp-engine` with the new wrapper module from `lp-model`. This includes updating `from_slice` usage in file loading code.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### Step 1: Update imports

Find all files in `lp-engine/src/` that use `serde_json`:

1. **`lp-engine/src/project/loader.rs`**:
   - Replace `serde_json::from_slice` with `lp_model::json::from_slice`
   - Update error handling - the error type should convert automatically with `?`

2. **`lp-engine/src/project/runtime.rs`**:
   - Replace all `serde_json::from_slice` calls with `lp_model::json::from_slice`
   - Update error handling if needed

### Step 2: Verify dependencies

Ensure `lp-engine/Cargo.toml` has `lp-model` as a dependency (it should already).

### Step 3: Update error handling (if needed)

If any code uses `serde_json::Error` directly, update to use `lp_model::json::Error`:

```rust
// Before
use serde_json::Error;

// After
use lp_model::json::Error;
```

### Step 4: Verify all tests pass

Run all tests to ensure nothing broke:

```bash
cd lp-core/lp-engine
cargo test
```

## Validate

Run the following commands to validate:

```bash
cd lp-core/lp-engine
cargo check
cargo test
```

Expected results:
- Code compiles without errors
- All tests pass
- No `serde_json` imports remain in `lp-engine/src/` (except possibly in comments)
