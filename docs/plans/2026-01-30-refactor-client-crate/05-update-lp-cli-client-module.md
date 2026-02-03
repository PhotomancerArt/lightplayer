# Phase 5: Update lp-cli client module

## Scope of phase

Update `lp-cli/src/client/mod.rs` to re-export from `lp-client`, and update `local_server.rs` and `client_connect.rs` to use `lp-client` imports.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. Update `lp-cli/src/client/mod.rs`:

   ```rust
   // Re-export everything from lp-client for backward compatibility
   pub use lp_client::*;

   // CLI-specific modules
   pub mod local_server;
   pub mod client_connect;

   // Re-export CLI-specific types
   pub use local_server::LocalServerTransport;
   pub use client_connect::client_connect;
   ```

2. Update `lp-cli/src/client/local_server.rs`:
   - Change `use crate::client::transport::ClientTransport;` to `use lp_client::ClientTransport;`
   - Change `use crate::client::local::{AsyncLocalClientTransport, create_local_transport_pair};` to `use lp_client::{AsyncLocalClientTransport, create_local_transport_pair};`
   - Keep `use crate::server::{create_server, run_server_loop_async};` unchanged (lp-cli specific)

3. Update `lp-cli/src/client/client_connect.rs`:
   - Change `use crate::client::transport::ClientTransport;` to `use lp_client::ClientTransport;`
   - Change `use crate::client::local_server::LocalServerTransport;` to `use crate::client::local_server::LocalServerTransport;` (stays the same, it's in same module)
   - Change `use crate::client::specifier::HostSpecifier;` to `use lp_client::HostSpecifier;`
   - Change `use crate::client::transport_ws::WebSocketClientTransport;` to `use lp_client::WebSocketClientTransport;`
   - Update doctest example: change `use lp_cli::client::{client_connect, specifier::HostSpecifier};` to use `lp_client::HostSpecifier` and `lp_cli::client::client_connect`

4. Update `lp-cli/src/server/create_server.rs`:
   - Change `use crate::client::local::create_local_transport_pair;` to `use lp_client::create_local_transport_pair;`

## Validate

Run `cargo check --package lp-cli` to verify all imports are correct and the module structure works.
