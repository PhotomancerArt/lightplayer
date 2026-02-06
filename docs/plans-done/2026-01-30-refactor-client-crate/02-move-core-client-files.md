# Phase 2: Move core client files to lp-client

## Scope of phase

Move the core client files from `lp-cli/src/client` to `lp-core/lp-client/src` and update internal imports.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. Copy files from `lp-cli/src/client` to `lp-client/src`:
   - `client.rs`
   - `transport.rs`
   - `transport_ws.rs`
   - `local.rs`
   - `specifier.rs`

2. Update imports in each file:

   **client.rs**:
   - Change `use crate::client::transport::ClientTransport;` to `use crate::transport::ClientTransport;`

   **transport_ws.rs**:
   - Change `use crate::client::transport::ClientTransport;` to `use crate::transport::ClientTransport;`

   **local.rs**:
   - Change `use crate::client::transport::ClientTransport;` to `use crate::transport::ClientTransport;`

   **client_connect.rs** (will stay in lp-cli, but note for later):
   - Will need to import from `lp_client` instead

3. Update doctest examples in files:
   - Change `use lp_cli::client::...` to `use lp_client::...` in doc comments
   - Update any other references to `lp_cli` in documentation

4. Keep tests in each file - they should continue to work after import updates.

## Validate

Run `cargo check --package lp-client` to verify all files compile and imports are correct.
