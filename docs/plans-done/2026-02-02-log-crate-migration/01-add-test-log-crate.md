# Phase 1: Add test-log Crate Dependency

## Scope of phase

Add `test-log` crate to the workspace for automatic test logger initialization. This enables all tests to use `#[test_log::test]` attribute for automatic `env_logger` initialization.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Add test-log to Workspace Cargo.toml

**File**: `Cargo.toml`

Add `test-log` to the workspace dependencies:

```toml
[workspace.dependencies]
# ... existing dependencies ...
test-log = "0.2"
env_logger = "0.11"
```

Note: `test-log` uses `env_logger` internally, but we also need `env_logger` directly for std applications.

### 2. Add test-log to Crate Cargo.toml Files

Add `test-log` as a dev-dependency to crates that have tests:

**Crates that need test-log**:
- `lp-core/lp-shared/Cargo.toml` (if it has tests)
- `lp-core/lp-client/Cargo.toml`
- `lp-core/lp-server/Cargo.toml`
- `lp-fw/fw-tests/Cargo.toml`
- `lp-glsl/lp-glsl-filetests/Cargo.toml`
- Any other crate with tests

**Example** (for `lp-core/lp-shared/Cargo.toml`):

```toml
[dev-dependencies]
test-log = { workspace = true }
env_logger = { workspace = true }
```

### 3. Update Test Examples

Add a comment or example showing how to use `test-log` in tests. This can be in a README or as a comment in a test file.

**Example usage**:

```rust
use test_log::test;

#[test]
fn my_test() {
    log::debug!("This will show if RUST_LOG=debug");
    assert_eq!(2 + 2, 4);
}

#[tokio::test]
#[test_log::test]
async fn my_async_test() {
    log::info!("Async test logging");
}
```

## Tests

No tests needed for this phase - we're just adding a dependency. The actual usage will be validated in later phases when we update tests.

## Validate

Run from workspace root:

```bash
cargo check --workspace
```

Ensure:
- All crates compile successfully
- `test-log` dependency is available
- No dependency conflicts
