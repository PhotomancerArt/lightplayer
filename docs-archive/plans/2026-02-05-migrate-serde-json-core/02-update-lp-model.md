# Phase 2: Update lp-model to Use Wrapper

## Scope of Phase

Replace all `serde_json` usage in `lp-model` with the new wrapper module. This includes updating imports and usage in source files and tests.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### Step 1: Update imports in source files

Find all files in `lp-model/src/` that use `serde_json`:

1. **`lp-model/src/server/config.rs`**:
   - Replace `serde_json::to_string` with `crate::json::to_string`
   - Replace `serde_json::from_str` with `crate::json::from_str`

2. Check for any other source files using `serde_json` directly

### Step 2: Update imports in test files

Find all test files that use `serde_json`:

1. **`lp-model/src/message.rs`** (tests):
   - Replace `serde_json::to_string` with `crate::json::to_string`
   - Replace `serde_json::from_str` with `crate::json::from_str`

2. **`lp-model/src/project/api.rs`** (tests):
   - Replace `serde_json::to_string` with `crate::json::to_string`
   - Replace `serde_json::from_str` with `crate::json::from_str`

3. **`lp-model/src/server/fs_api.rs`** (tests):
   - Replace all `serde_json::to_string` with `crate::json::to_string`
   - Replace all `serde_json::from_str` with `crate::json::from_str`

### Step 3: Update error handling (if needed)

If any code uses `serde_json::Error` directly, update to use `crate::json::Error`:

```rust
// Before
use serde_json::Error;

// After
use crate::json::Error;
```

### Step 4: Verify all tests pass

Run all tests to ensure nothing broke:

```bash
cd lp-core/lp-model
cargo test
```

## Validate

Run the following commands to validate:

```bash
cd lp-core/lp-model
cargo check
cargo test
```

Expected results:
- Code compiles without errors
- All tests pass
- No `serde_json` imports remain in `lp-model/src/` (except possibly in comments)
