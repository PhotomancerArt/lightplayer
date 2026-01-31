# Refactor Client to lp-core/lp-client

## Scope of Work

Refactor the client module from `lp-cli/src/client` into its own crate at `lp-core/lp-client` so it can be used by other crates in the workspace.

## Current State

The client module (`lp-cli/src/client`) contains:

- **client.rs**: `LpClient` struct with async methods for filesystem and project operations
- **client_connect.rs**: `client_connect()` function that creates transports based on `HostSpecifier`
- **transport.rs**: `ClientTransport` trait definition
- **transport_ws.rs**: WebSocket transport implementation
- **local.rs**: Local in-memory transport implementations (`AsyncLocalClientTransport`, `AsyncLocalServerTransport`, `create_local_transport_pair`)
- **local_server.rs**: `LocalServerTransport` that manages an in-memory server thread
- **specifier.rs**: `HostSpecifier` enum for parsing connection strings

### Dependencies

The client module depends on:
- `lp-model`: For message types (`ClientMessage`, `ServerMessage`, etc.)
- `lp-shared`: For `ServerTransport` trait (used in `local.rs`)
- `tokio`: For async runtime
- `anyhow`: For error handling
- `serde_json`: For serialization (WebSocket transport)
- `tokio-tungstenite`: For WebSocket support
- `futures-util`: For WebSocket stream handling

### Problem: LocalServerTransport Dependency

`LocalServerTransport` (in `local_server.rs`) has a dependency on lp-cli-specific code:
- It imports `crate::server::{create_server, run_server_loop_async}` 
- `create_server` depends on `crate::commands::serve::init` which is lp-cli specific
- This creates a circular dependency if we want `lp-client` to be usable without lp-cli

## Questions

1. **How should we handle `LocalServerTransport`?**
   - ✅ **Option D: Keep `LocalServerTransport` in lp-cli and only move the core client code**
   - Decision: `LocalServerTransport` is a convenience wrapper for CLI/testing use cases. The core client functionality (LpClient, transports, etc.) will be moved to `lp-client`, while `LocalServerTransport` stays in lp-cli and depends on `lp-client`.

## Decision

**Keep `LocalServerTransport` in lp-cli**: This is a convenience wrapper that creates a local server thread, which is more of a CLI/testing utility. The core client functionality will be moved to `lp-client`, and `LocalServerTransport` will depend on `lp-client` instead of containing the client code directly.
