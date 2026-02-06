# Phase 6: Update imports throughout lp-cli

## Scope of phase

Update all files in `lp-cli` that import from `crate::client` to use `lp_client` directly or through the re-exports in `lp-cli::client`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

Files that need updating (from grep results):

1. **lp-cli/src/debug_ui/ui.rs**:
   - Change `use crate::client::{LpClient, serializable_response_to_project_response};` to `use lp_client::{LpClient, serializable_response_to_project_response};`
   - Or keep using `crate::client::` since it re-exports from `lp_client`

2. **lp-cli/src/commands/dev/sync.rs**:
   - Change `use crate::client::LpClient;` to `use lp_client::LpClient;` or keep `crate::client::LpClient`

3. **lp-cli/src/commands/dev/push_project.rs**:
   - Change `use crate::client::LpClient;` to `use lp_client::LpClient;` or keep `crate::client::LpClient`

4. **lp-cli/src/commands/dev/pull_project.rs**:
   - Change `use crate::client::LpClient;` to `use lp_client::LpClient;` or keep `crate::client::LpClient`

5. **lp-cli/src/commands/dev/handler.rs**:
   - Change `use crate::client::{LpClient, client_connect, specifier::HostSpecifier};` to:
     - `use lp_client::{LpClient, HostSpecifier};`
     - `use crate::client::client_connect;` (or `use lp_cli::client::client_connect;`)

6. **lp-cli/src/commands/dev/fs_loop.rs**:
   - Change `use crate::client::LpClient;` to `use lp_client::LpClient;` or keep `crate::client::LpClient`

7. **lp-cli/src/commands/dev/async_client.rs**:
   - Change `pub use crate::client::{LpClient, serializable_response_to_project_response};` to:
     - `pub use lp_client::{LpClient, serializable_response_to_project_response};`

**Note**: Since `lp-cli/src/client/mod.rs` re-exports everything from `lp_client`, we can keep using `crate::client::` for backward compatibility. However, for clarity and to make the dependency explicit, we should prefer `lp_client::` for types that come from lp-client, and `crate::client::` only for CLI-specific things like `client_connect`.

## Validate

Run `cargo check --package lp-cli` to verify all imports work correctly.

Run `cargo test --package lp-cli` to ensure tests still pass.
