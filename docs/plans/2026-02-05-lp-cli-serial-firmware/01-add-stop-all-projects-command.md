# Phase 1: Add StopAllProjects Command

## Scope of phase

Add a new `StopAllProjects` command to the protocol and implement it on both client and server sides. This will allow clients to stop all loaded projects at once before pushing a new one.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update Protocol Types

**File**: `lp-core/lp-model/src/message.rs`

Add `StopAllProjects` variant to `ClientRequest` enum:

```rust
pub enum ClientRequest {
    // ... existing variants ...
    /// Stop all loaded projects
    StopAllProjects,
}
```

**File**: `lp-core/lp-model/src/server/api.rs`

Add `StopAllProjects` variant to `ServerMsgBody` enum:

```rust
pub enum ServerMsgBody {
    // ... existing variants ...
    /// Response to StopAllProjects request
    StopAllProjects,
}
```

### 2. Implement Server Handler

**File**: `lp-core/lp-server/src/project_manager.rs`

Add `unload_all_projects()` method:

```rust
/// Unload all loaded projects
///
/// Removes all projects from memory but doesn't delete them from the filesystem.
pub fn unload_all_projects(&mut self) {
    self.projects.clear();
    self.name_to_handle.clear();
    // Note: next_handle_id is not reset - handles continue incrementing
}
```

**File**: `lp-core/lp-server/src/handlers.rs`

Add handler function:

```rust
/// Handle a StopAllProjects request
fn handle_stop_all_projects(
    project_manager: &mut ProjectManager,
) -> Result<ServerMessagePayload, ServerError> {
    project_manager.unload_all_projects();
    Ok(ServerMessagePayload::StopAllProjects)
}
```

Update `handle_client_message()` to route `StopAllProjects`:

```rust
let response = match msg {
    // ... existing matches ...
    lp_model::ClientRequest::StopAllProjects => {
        handle_stop_all_projects(project_manager)?
    }
};
```

### 3. Implement Client Method

**File**: `lp-core/lp-client/src/client.rs`

Add `stop_all_projects()` method:

```rust
/// Stop all loaded projects on the server
///
/// # Returns
///
/// * `Ok(())` if all projects were stopped successfully
/// * `Err` if the request failed
pub async fn stop_all_projects(&self) -> Result<(), ClientError> {
    let request_id = self.next_request_id();
    let msg = ClientMessage {
        id: request_id,
        msg: ClientRequest::StopAllProjects,
    };

    self.send(msg).await?;
    let response = self.receive().await?;

    if response.id != request_id {
        return Err(ClientError::Protocol(format!(
            "Response ID mismatch: expected {}, got {}",
            request_id, response.id
        )));
    }

    match response.msg {
        ServerMessagePayload::StopAllProjects => Ok(()),
        _ => Err(ClientError::Protocol(format!(
            "Unexpected response type: expected StopAllProjects, got {:?}",
            response.msg
        ))),
    }
}
```

### 4. Add Tests

**File**: `lp-core/lp-server/tests/stop_all_projects.rs` (NEW)

Create test file:

```rust
use lp_server::{LpServer, ProjectManager};
use lp_shared::fs::LpFsMemory;
use lp_shared::output::OutputProvider;
use lp_model::AsLpPath;

// Test that unload_all_projects clears all projects
#[test]
fn test_unload_all_projects() {
    // Setup: create project manager and load some projects
    // Then call unload_all_projects and verify all are cleared
}
```

**File**: `lp-core/lp-client/tests/stop_all_projects.rs` (NEW)

Create test file:

```rust
use lp_client::LpClient;
use lp_client::transport::memory::MemoryTransport;

// Test that stop_all_projects sends correct message and handles response
#[tokio::test]
async fn test_stop_all_projects() {
    // Setup: create client with memory transport
    // Call stop_all_projects and verify correct message sent
    // Verify response is handled correctly
}
```

## Validate

Run the following commands to validate the phase:

```bash
cd lp-core
cargo check --workspace
cargo test --workspace
```

Fix any warnings or errors before proceeding.
