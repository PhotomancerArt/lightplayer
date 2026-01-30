# Design: lp-client Cleanup and Refactor

## Overview

Delete the `lp-client` crate entirely and rebuild `LpClient` as a standalone client in `lp-cli`. Create a new async `ClientTransport` trait in `lp-cli` built for our needs.

## Goals

1. Delete `lp-client` crate completely
2. Create async `ClientTransport` trait in `lp-cli`
3. Rebuild `LpClient` from scratch as standalone
4. Remove all references to `lp-client` from `lp-cli`
5. Get `lp-cli` building again

## File Structure

```
lp-app/apps/lp-cli/src/client/
├── mod.rs                          # UPDATE: Remove async_client, async_transport references
├── client_connect.rs               # KEEP: No changes needed
├── local.rs                        # KEEP: Already has local transport
├── local_server.rs                 # KEEP: No changes needed
├── specifier.rs                    # KEEP: No changes needed
├── transport_ws.rs                 # KEEP: No changes needed
├── transport.rs                   # NEW: ClientTransport trait (async)
└── async_client.rs                 # NEW: Rebuild AsyncLpClient from scratch
```

## Types and Functions

### transport.rs (NEW)

```rust
// Async ClientTransport trait for lp-cli
pub trait ClientTransport: Send {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError>;
    async fn receive(&mut self) -> Result<ServerMessage, TransportError>;
    async fn close(&mut self) -> Result<(), TransportError>;
}
```

### client.rs (NEW)

```rust
// Standalone LpClient - no dependency on lp-client
pub struct LpClient {
    transport: Arc<dyn ClientTransport>,
    next_request_id: Arc<AtomicU64>,
}

impl LpClient {
    pub fn new(transport: Arc<dyn ClientTransport>) -> Self;
    
    // Filesystem operations
    pub async fn fs_read(&self, path: &str) -> Result<Vec<u8>>;
    pub async fn fs_write(&self, path: &str, data: Vec<u8>) -> Result<()>;
    pub async fn fs_delete_file(&self, path: &str) -> Result<()>;
    pub async fn fs_list_dir(&self, path: &str, recursive: bool) -> Result<Vec<String>>;
    
    // Project operations
    pub async fn project_load(&self, path: &str) -> Result<ProjectHandle>;
    pub async fn project_unload(&self, handle: ProjectHandle) -> Result<()>;
    pub async fn project_sync_internal(
        &self,
        handle: ProjectHandle,
        since_frame: Option<FrameId>,
        detail_specifier: ApiNodeSpecifier,
    ) -> Result<SerializableProjectResponse>;
    pub async fn project_list_available(&self) -> Result<Vec<AvailableProject>>;
    pub async fn project_list_loaded(&self) -> Result<Vec<LoadedProject>>;
}

// Helper function
pub fn serializable_response_to_project_response(
    response: SerializableProjectResponse,
) -> Result<ProjectResponse, Error>;
```

## Implementation Notes

1. **Request ID generation**: Use `Arc<AtomicU64>` for thread-safe request ID counter
2. **Message creation**: Create `ClientMessage` directly in each method
3. **Response extraction**: Extract responses directly from `ServerMessage` - no need for separate extract functions
4. **Error handling**: Use `anyhow::Result` for simplicity
5. **Transport**: The `ClientTransport` trait is async and designed for tokio

## Migration Strategy

1. Remove `lp-client` dependency from `lp-cli/Cargo.toml`
2. Delete `lp-client` crate directory
3. Remove `lp-client` from workspace `Cargo.toml`
4. Create new `transport.rs` with `ClientTransport` trait
5. Rebuild `client.rs` from scratch
6. Update `mod.rs` to export new modules
7. Update all call sites to use new API (rename AsyncLpClient -> LpClient)
8. Fix compilation errors

## Success Criteria

- `lp-client` crate completely deleted
- `lp-cli` compiles without errors
- `LpClient` works with existing code (handler.rs, push_project.rs, etc.)
- All tests pass (or are removed if they depended on lp-client)
