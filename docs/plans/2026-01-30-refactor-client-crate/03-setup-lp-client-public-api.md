# Phase 3: Set up lp-client public API

## Scope of phase

Create the public API for `lp-client` by updating `lib.rs` with proper module declarations and exports.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. Update `lp-client/src/lib.rs`:

   ```rust
   //! LightPlayer client library.
   //!
   //! Provides client-side functionality for communicating with LpServer.
   //! Includes transport implementations and the main LpClient struct.

   pub mod client;
   pub mod transport;
   pub mod transport_ws;
   pub mod local;
   pub mod specifier;

   // Re-export main types
   pub use client::{LpClient, serializable_response_to_project_response};
   pub use transport::ClientTransport;
   pub use specifier::HostSpecifier;
   pub use transport_ws::WebSocketClientTransport;
   pub use local::{
       AsyncLocalClientTransport,
       AsyncLocalServerTransport,
       create_local_transport_pair,
   };
   ```

2. Ensure all public types are properly exported:
   - `LpClient` - main client struct
   - `ClientTransport` - transport trait
   - `HostSpecifier` - connection specifier enum
   - `WebSocketClientTransport` - WebSocket transport implementation
   - `AsyncLocalClientTransport` - local client transport
   - `AsyncLocalServerTransport` - local server transport (for testing)
   - `create_local_transport_pair` - helper function
   - `serializable_response_to_project_response` - helper function

3. Verify all modules compile and are accessible:
   - Check that `pub mod` declarations match actual files
   - Ensure no private types are accidentally exposed

## Validate

Run `cargo check --package lp-client` to verify the public API is correctly set up and all exports are accessible.
