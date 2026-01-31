# Phase 7: Cleanup and validation

## Scope of phase

Remove moved files from `lp-cli/src/client`, fix any remaining issues, run tests, and ensure everything compiles and works correctly.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. Remove moved files from `lp-cli/src/client`:
   - Delete `client.rs` (moved to lp-client)
   - Delete `transport.rs` (moved to lp-client)
   - Delete `transport_ws.rs` (moved to lp-client)
   - Delete `local.rs` (moved to lp-client)
   - Delete `specifier.rs` (moved to lp-client)

2. Verify remaining files in `lp-cli/src/client`:
   - `mod.rs` - re-exports and CLI-specific module declarations
   - `local_server.rs` - CLI-specific (depends on server creation)
   - `client_connect.rs` - CLI-specific (depends on LocalServerTransport)

3. Check for any remaining references to moved files:
   - Search for any imports that might still reference the old paths
   - Fix any broken imports

4. Update any doctests or examples:
   - Ensure all doctest examples use correct import paths
   - Update any README or documentation that references the client module

5. Run formatting:
   - Run `cargo +nightly fmt` on all changed files

6. Fix warnings:
   - Address any compiler warnings
   - Remove any unused imports
   - Fix any dead code warnings (unless they're intentionally marked with `#[allow(dead_code)]`)

## Validate

Run the following commands to ensure everything works:

```bash
# Check both crates compile
cargo check --package lp-client
cargo check --package lp-cli

# Run tests
cargo test --package lp-client
cargo test --package lp-cli

# Check workspace builds
cargo check --workspace

# Format code
cargo +nightly fmt
```

Fix any errors or warnings that appear.
