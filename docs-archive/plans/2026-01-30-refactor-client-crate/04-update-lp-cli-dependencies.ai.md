# Phase 4: Update lp-cli to depend on lp-client

## Scope of phase

Add `lp-client` as a dependency to `lp-cli` and add it to the workspace.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. Update `lp-cli/Cargo.toml`:
   - Add dependency: `lp-client = { path = "../lp-core/lp-client" }`
   - Keep existing dependencies (they may still be needed for other parts of lp-cli)

2. Verify workspace configuration:
   - Ensure `lp-core/lp-client` is in the workspace `members` array (done in Phase 1)
   - Ensure `lp-core/lp-client` is in the workspace `default-members` array (done in Phase 1)

3. Test that lp-cli can import lp-client:
   - Temporarily add a test import in `lp-cli/src/lib.rs` or `main.rs` to verify the dependency works
   - Remove the test import after verification

## Validate

Run `cargo check --package lp-cli` to verify lp-cli can successfully depend on lp-client.
