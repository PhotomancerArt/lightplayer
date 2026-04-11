# Design: Refactor Client to lp-core/lp-client

## Scope of Work

Refactor the client module from `lp-cli/src/client` into its own crate at `lp-core/lp-client` so it can be used by other crates in the workspace. Keep `LocalServerTransport` and `client_connect` in lp-cli since they depend on lp-cli-specific server creation code.

## File Structure

```
lp-core/
└── lp-client/                    # NEW: New client crate
    ├── Cargo.toml                # NEW: Client crate dependencies
    └── src/
        ├── lib.rs                # NEW: Public API exports
        ├── client.rs             # MOVED: LpClient struct and methods
        ├── transport.rs          # MOVED: ClientTransport trait
        ├── transport_ws.rs       # MOVED: WebSocket transport
        ├── local.rs               # MOVED: Local transport implementations
        └── specifier.rs          # MOVED: HostSpecifier enum

lp-cli/
└── src/
    ├── client/                   # UPDATE: Reduced to CLI-specific code
    │   ├── mod.rs                # UPDATE: Re-exports from lp-client, adds LocalServerTransport
    │   ├── local_server.rs       # KEEP: Stays in lp-cli (depends on server creation)
    │   └── client_connect.rs     # KEEP: Stays in lp-cli (depends on LocalServerTransport)
    └── ...                       # UPDATE: Update imports to use lp_client crate
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      lp-client crate                         │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐     │
│  │   LpClient   │  │  Transport   │  │  Specifier   │     │
│  │              │  │   Traits     │  │              │     │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘     │
│         │                 │                 │              │
│         │         ┌───────┴───────┐         │              │
│         │         │  Transports   │         │              │
│         │         │               │         │              │
│         │         │ ┌──────────┐ │         │              │
│         └─────────┼─│ WebSocket│ │         │              │
│                   │ └──────────┘ │         │              │
│                   │ ┌──────────┐ │         │              │
│                   │ │  Local   │ │         │              │
│                   │ └──────────┘ │         │              │
│                   └─────────────┘         │              │
└────────────────────────────────────────────┴──────────────┘
                                             │
                                             │ depends on
                                             ▼
┌─────────────────────────────────────────────────────────────┐
│                      lp-cli crate                            │
│                                                              │
│  ┌──────────────────┐  ┌──────────────────┐               │
│  │ LocalServer      │  │ client_connect   │               │
│  │ Transport        │  │                  │               │
│  │                  │  │                  │               │
│  │ (creates server  │  │ (uses            │               │
│  │  thread)         │  │  LocalServer      │               │
│  └──────────────────┘  │  Transport)      │               │
│                        └──────────────────┘               │
└─────────────────────────────────────────────────────────────┘
```

## Main Components

### lp-client crate

**Core Types:**

- `LpClient`: Main client struct with async methods for filesystem and project operations
- `ClientTransport`: Trait for transport implementations
- `HostSpecifier`: Enum for parsing connection strings

**Transport Implementations:**

- `WebSocketClientTransport`: WebSocket-based transport
- `AsyncLocalClientTransport`: Local in-memory client transport
- `AsyncLocalServerTransport`: Local in-memory server transport (for testing)
- `create_local_transport_pair()`: Helper to create connected local transports

**Dependencies:**

- `lp-model`: Message types
- `lp-shared`: ServerTransport trait (for AsyncLocalServerTransport)
- `tokio`: Async runtime
- `anyhow`: Error handling
- `serde_json`: Serialization
- `tokio-tungstenite`: WebSocket support
- `futures-util`: Stream utilities

### lp-cli crate (updated)

**CLI-Specific Client Code:**

- `LocalServerTransport`: Wraps server creation and provides ClientTransport interface
- `client_connect()`: Factory function that creates transports based on HostSpecifier

**Changes:**

- Import `lp-client` crate as dependency
- Re-export `lp-client` types through `lp-cli::client` module for backward compatibility
- Keep `LocalServerTransport` and `client_connect` in lp-cli

## Public API

### lp-client crate

```rust
pub use client::LpClient;
pub use transport::ClientTransport;
pub use specifier::HostSpecifier;
pub use local::{AsyncLocalClientTransport, AsyncLocalServerTransport, create_local_transport_pair};
pub use transport_ws::WebSocketClientTransport;
```

### lp-cli crate (backward compatibility)

```rust
pub mod client {
    // Re-export everything from lp-client
    pub use lp_client::*;

    // Add CLI-specific types
    pub use local_server::LocalServerTransport;
    pub use client_connect::client_connect;
}
```

## Migration Strategy

1. Create new `lp-client` crate with core client code
2. Update `lp-cli` to depend on `lp-client`
3. Move files from `lp-cli/src/client` to `lp-client/src`
4. Keep `local_server.rs` and `client_connect.rs` in `lp-cli/src/client`
5. Update `lp-cli/src/client/mod.rs` to re-export from `lp-client` and add CLI-specific types
6. Update all imports in `lp-cli` to use `lp_client` directly or through re-exports
7. Add `lp-client` to workspace members
