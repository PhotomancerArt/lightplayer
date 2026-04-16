# Phase 7: Remove serde_json Dependencies

## Scope of Phase

Remove `serde_json` dependencies from all crate `Cargo.toml` files and the workspace `Cargo.toml`. Verify everything still compiles and works.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### Step 1: Remove from lp-model

Update `lp-core/lp-model/Cargo.toml`:
- Remove `serde_json = { workspace = true }`

### Step 2: Remove from lp-shared

Update `lp-core/lp-shared/Cargo.toml`:
- Remove `serde_json = { workspace = true }`

### Step 3: Remove from lp-engine

Update `lp-core/lp-engine/Cargo.toml`:
- Remove `serde_json = { workspace = true }`

### Step 4: Remove from lp-client

Update `lp-core/lp-client/Cargo.toml`:
- Remove `serde_json = "1"`

### Step 5: Remove from lp-server

Update `lp-core/lp-server/Cargo.toml`:
- Remove `serde_json = { workspace = true, default-features = false, features = ["alloc"] }`

### Step 6: Remove from workspace Cargo.toml

Update root `Cargo.toml`:
- Find `serde_json` in workspace dependencies
- Remove it (or comment it out if other parts of the codebase still use it)

### Step 7: Verify compilation

Run cargo check on all affected crates:

```bash
cd lp-core/lp-model && cargo check
cd lp-core/lp-shared && cargo check
cd lp-core/lp-engine && cargo check
cd lp-core/lp-client && cargo check
cd lp-core/lp-server && cargo check
```

### Step 8: Run full test suite

Run tests to ensure everything still works:

```bash
cd lp-core
cargo test --workspace
```

## Validate

Run the following commands to validate:

```bash
cd lp-core
cargo check --workspace
cargo test --workspace
```

Expected results:
- All crates compile without errors
- All tests pass
- No references to `serde_json` in dependencies (except possibly in comments or other parts of codebase outside lp-core)
