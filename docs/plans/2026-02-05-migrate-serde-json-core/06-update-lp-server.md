# Phase 6: Update lp-server to Use Wrapper

## Scope of Phase

Replace all `serde_json` usage in `lp-server` with the new wrapper module from `lp-model`.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### Step 1: Update imports

Find all files in `lp-server/src/` that use `serde_json`:

1. Check `lp-server/src/` for any `serde_json` usage
2. Replace with `lp_model::json::*` equivalents
3. Update error handling if needed

### Step 2: Verify dependencies

Ensure `lp-server/Cargo.toml` has `lp-model` as a dependency (it should already).

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
cd lp-core/lp-server
cargo test
```

## Validate

Run the following commands to validate:

```bash
cd lp-core/lp-server
cargo check
cargo test
```

Expected results:
- Code compiles without errors
- All tests pass
- No `serde_json` imports remain in `lp-server/src/` (except possibly in comments)
