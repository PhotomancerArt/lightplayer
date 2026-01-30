# Phase 3: Rebuild LpClient from Scratch

## Description

Create a new standalone `LpClient` in `lp-cli` that implements all the methods needed by the existing code.

## Tasks

1. Create `lp-app/apps/lp-cli/src/client/client.rs`:
   - Define `LpClient` struct with:
     - `transport: Arc<dyn ClientTransport>`
     - `next_request_id: Arc<AtomicU64>`
   - Implement `new(transport: Arc<dyn ClientTransport>) -> Self`
   - Implement filesystem methods:
     - `async fn fs_read(&self, path: &str) -> Result<Vec<u8>>`
     - `async fn fs_write(&self, path: &str, data: Vec<u8>) -> Result<()>`
     - `async fn fs_delete_file(&self, path: &str) -> Result<()>`
     - `async fn fs_list_dir(&self, path: &str, recursive: bool) -> Result<Vec<String>>`
   - Implement project methods:
     - `async fn project_load(&self, path: &str) -> Result<ProjectHandle>`
     - `async fn project_unload(&self, handle: ProjectHandle) -> Result<()>`
     - `async fn project_sync_internal(&self, handle: ProjectHandle, since_frame: Option<FrameId>, detail_specifier: ApiNodeSpecifier) -> Result<SerializableProjectResponse>`
     - `async fn project_list_available(&self) -> Result<Vec<AvailableProject>>`
     - `async fn project_list_loaded(&self) -> Result<Vec<LoadedProject>>`
   - Each method should:
     - Generate request ID using atomic counter
     - Create `ClientMessage` with appropriate `ClientRequest`
     - Send via transport and await response
     - Extract result from `ServerMessage` response
     - Return typed result or error

2. Add helper function `serializable_response_to_project_response`:
   - Convert `SerializableProjectResponse` to `ProjectResponse`
   - Handle node detail conversion

3. Update `lp-app/apps/lp-cli/src/client/mod.rs`:
   - Add `pub mod client;`
   - Add `pub use client::{LpClient, serializable_response_to_project_response};`

## Success Criteria

- `LpClient` struct exists with all required methods
- Methods create `ClientMessage` and extract responses correctly
- Request ID generation works correctly
- Helper function exists for response conversion
- Code compiles (call sites will still reference `AsyncLpClient`, that's next phase)
