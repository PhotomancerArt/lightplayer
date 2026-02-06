# Phase 3: Update lp-shared to Use Wrapper

## Scope of Phase

Replace all `serde_json` usage in `lp-shared` with the new wrapper module from `lp-model`.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### Step 1: Update imports

Find all files in `lp-shared/src/` that use `serde_json`:

1. **`lp-shared/src/project/builder.rs`**:
   - Replace `serde_json::to_string` with `lp_model::json::to_string`
   - Update error handling if needed (should work with `?` operator)

### Step 2: Verify dependencies

Ensure `lp-shared/Cargo.toml` has `lp-model` as a dependency (it should already).

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
cd lp-core/lp-shared
cargo test
```

## Validate

Run the following commands to validate:

```bash
cd lp-core/lp-shared
cargo check
cargo test
```

Expected results:
- Code compiles without errors
- All tests pass
- No `serde_json` imports remain in `lp-shared/src/` (except possibly in comments)
