# Design: LP Client-Server Sync

## Overview

This plan implements client-server synchronization for LightPlayer, enabling `lp-client` to sync
with `lp-server` instances. The design uses frame-based incremental updates, handle-based runtime
identification, and a transport-agnostic client library.

## Architecture Decisions

### Handle-Based Runtime IDs

- Node HashMaps use `NodeHandle` (i32) as keys instead of path-based IDs
- `HashMap<NodeHandle, NodeRuntime>` serves as the registry (no separate registry needed)
- Each node runtime stores its path for lookups
- Sequential handle assignment via `next_handle: i32` counter
- Handles are runtime-specific (reset on reload)

### Frame-Based Versioning

- Frame ID increments every `ProjectRuntime::update()` call
- Per-node tracking:
    - `created_frame: FrameId` - when node was created
    - `last_config_frame: FrameId` - when config last changed
    - `last_state_frame: FrameId` - when state last changed
- Frame IDs reset to 0 on project reload

### Message Protocol

- Request IDs (client-generated u64) for correlation
- Message envelope enum wrapping requests/responses/logs
- JSON serialization with compression (gzip/zlib)
- Polling-based transport with message buffering

## File Structure

```
lp-engine/src/project/runtime.rs          # MODIFY: Add handle/frame tracking
lp-engine/src/nodes/*/runtime.rs         # MODIFY: Add handle, path, frame fields

lp-shared/src/project/api.rs              # MODIFY: Fix NodeSpecifier to use NodeHandle
lp-shared/src/transport.rs                # NEW: ClientTransport trait
lp-shared/src/message.rs                  # NEW: Message envelope enum

lp-client/src/lib.rs                      # NEW: Main client library
lp-client/src/client.rs                   # NEW: LpClient struct
lp-client/src/project.rs                  # NEW: RemoteProject state structures
lp-client/src/sync.rs                     # NEW: Sync logic

lp-server/src/project.rs                  # MODIFY: Add GetChanges handler
lp-server/src/handlers.rs                 # NEW: ProjectRequest handlers
```

## Code Structure

### ProjectRuntime Changes

```rust
// lp-engine/src/project/runtime.rs
pub struct ProjectRuntime {
    uid: String,
    frame_time: FrameTime,
    current_frame: FrameId,              // NEW
    next_handle: i32,                    // NEW
    textures: HashMap<NodeHandle, TextureNodeRuntime>,      // CHANGED: was TextureId
    shaders: HashMap<NodeHandle, ShaderNodeRuntime>,       // CHANGED: was ShaderId
    fixtures: HashMap<NodeHandle, FixtureNodeRuntime>,     // CHANGED: was FixtureId
    outputs: HashMap<NodeHandle, OutputNodeRuntime>,        // CHANGED: was OutputId
}

impl ProjectRuntime {
    // NEW methods:
    pub fn get_current_frame(&self) -> FrameId;
    pub fn get_changed_nodes_since(&self, since_frame: FrameId) -> Vec<NodeHandle>;
    pub fn get_node_detail(&self, handle: NodeHandle) -> Option<NodeDetail>;
    pub fn get_engine_stats(&self) -> EngineStats;
    pub fn get_all_node_handles(&self) -> Vec<NodeHandle>;

    // MODIFIED: init() assigns handles and tracks frames
    // MODIFIED: update() increments current_frame
}
```

### Node Runtime Changes

```rust
// Base structure for all node runtimes
pub struct NodeRuntimeBase {
    handle: NodeHandle,                  // NEW
    path: String,                         // NEW
    created_frame: FrameId,               // NEW
    last_config_frame: FrameId,           // NEW
    last_state_frame: FrameId,           // NEW
}

// Each node runtime (TextureNodeRuntime, ShaderNodeRuntime, etc.) includes NodeRuntimeBase
// And stores NodeHandle references instead of path-based IDs
```

### Message Protocol

```rust
// lp-shared/src/message.rs
pub enum Message {
    Request {
        id: u64,
        request: ServerRequest,
    },
    Response {
        id: u64,
        response: ServerResponse,
    },
    Log {
        level: LogLevel,
        message: String,
    },
    // ... other server-sent message types
}
```

### Transport Trait

```rust
// lp-shared/src/transport.rs
pub trait ClientTransport {
    fn send_message(&mut self, message: &Message) -> Result<(), Error>;
    fn receive_message(&mut self) -> Result<Option<Message>, Error>;
    // Returns None if no message available (non-blocking)
}
```

### Client Structure

```rust
// lp-client/src/client.rs
pub struct LpClient<T: ClientTransport> {
    transport: T,
    projects: HashMap<ProjectHandle, RemoteProject>,
    next_request_id: u64,
}

impl<T: ClientTransport> LpClient<T> {
    pub fn new(transport: T) -> Self;

    // Server-level operations
    pub fn list_projects(&mut self) -> Result<Vec<ProjectInfo>, Error>;
    pub fn create_project(&mut self, path: String) -> Result<(), Error>;

    // Project loading/unloading
    pub fn load_project(&mut self, path: String) -> Result<ProjectHandle, Error>;
    pub fn unload_project(&mut self, handle: ProjectHandle) -> Result<(), Error>;

    /// Update a project's state from the server, including any files changed
    /// Including any config changes
    pub fn update_project(&mut self, handle: ProjectHandle) -> Result<&RemoteProject, Error>;

    /// Get the last known state of a project (without updating it)
    pub fn get_project(&self, handle: ProjectHandle) -> Option<&RemoteProject>;

    // Internal: handle incoming messages
    fn process_messages(&mut self) -> Result<(), Error>;
}

// lp-client/src/project.rs
pub struct ProjectInfo {
    path: String,
}

pub struct RemoteProject {
    path: String,
    config: ProjectConfig,
    last_frame_id: FrameId,
    watched_nodes: HashSet<NodeHandle>,
    nodes: HashMap<NodeHandle, RemoteNode>,
}

pub struct RemoteNode {
    path: String,
    config: NodeConfig,

    /// Last frame the config was updated, used to know to reload the config from disk
    /// if it's updated
    config_ver: FrameId,

    /// Last frame the state was updated
    state_ver: FrameId,
    detail: Option<NodeDetail>,
}
```

### Server Handler

```rust
// lp-server/src/handlers.rs
impl ProjectManager {
    pub fn handle_project_request(
        &mut self,
        handle: ProjectHandle,
        request: ProjectRequest,
    ) -> Result<ProjectResponse, Error> {
        match request {
            ProjectRequest::GetChanges { since_frame, detail_specifier } => {
                let runtime = self.get_runtime(handle)?;
                let current_frame = runtime.get_current_frame();
                let changed_nodes = runtime.get_changed_nodes_since(since_frame);

                let mut node_detail = HashMap::new();

                // Add changed nodes
                for handle in &changed_nodes {
                    if let Some(detail) = runtime.get_node_detail(*handle) {
                        node_detail.insert(*handle, detail);
                    }
                }

                // Add nodes matching detail_specifier
                match detail_specifier {
                    NodeSpecifier::All => {
                        for handle in runtime.get_all_node_handles() {
                            if !node_detail.contains_key(&handle) {
                                if let Some(detail) = runtime.get_node_detail(handle) {
                                    node_detail.insert(handle, detail);
                                }
                            }
                        }
                    }
                    NodeSpecifier::ByHandles(handles) => {
                        for handle in handles {
                            if !node_detail.contains_key(&handle) {
                                if let Some(detail) = runtime.get_node_detail(handle) {
                                    node_detail.insert(handle, detail);
                                }
                            }
                        }
                    }
                    NodeSpecifier::None => {}
                }

                let node_handles = runtime.get_all_node_handles();
                let engine_stats = runtime.get_engine_stats();

                Ok(ProjectResponse::GetChanges {
                    current_frame,
                    engine_stats,
                    node_detail,
                    node_handles,
                })
            }
        }
    }
}
```

## New Types and Functions Summary

**New Types:**

- `Message` enum - Message envelope for requests/responses/logs
- `ClientTransport` trait - Transport abstraction for client-server communication
- `LpClient<T>` - Main client struct
- `RemoteProject` - Project state on client
- `RemoteNode` - Node state on client
- `NodeRuntimeBase` - Common fields for all node runtimes

**New Functions:**

- `ProjectRuntime::get_current_frame()` - Get current frame ID
- `ProjectRuntime::get_changed_nodes_since()` - Get nodes changed since frame (uses min of
  config/state frame)
- `ProjectRuntime::get_node_detail()` - Get full node detail by handle
- `ProjectRuntime::get_engine_stats()` - Get engine statistics
- `ProjectRuntime::get_all_node_handles()` - Get all current node handles
- `LpClient::update_project()` - Sync project state from server
- `LpClient::get_project()` - Get cached project state
- `LpClient::process_messages()` - Handle incoming messages

**Modified Functions:**

- `ProjectRuntime::init()` - Assign handles, track creation frames
- `ProjectRuntime::update()` - Increment frame ID, update state frames
- Node runtime `init()` methods - Store handle and path, track frames
- Node runtime update methods - Update `last_state_frame` when state changes

## Implementation Notes

### Handle Assignment

- Handles assigned sequentially starting from 0
- Assigned during `ProjectRuntime::init()` when nodes are created
- Stored in node runtime base structure

### Frame Tracking

- `current_frame` increments every `update()` call
- `created_frame` set during node initialization
- `last_config_frame` updated when config files change
- `last_state_frame` updated when runtime state changes (texture pixels, shader errors, etc.)

### Change Detection

- `get_changed_nodes_since()` uses `min(last_config_frame, last_state_frame)` to determine if node
  changed
- If either config or state changed since `since_frame`, node is included

### Client Sync Flow

1. Client calls `update_project(handle)`
2. Client sends `GetChanges` request with `since_frame = last_frame_id`
3. Server responds with changes
4. Client updates `RemoteProject` state:
    - Updates `last_frame_id` to `current_frame`
    - Updates/inserts nodes in `nodes` map
    - Removes nodes not in `node_handles` list
5. Returns reference to updated `RemoteProject`

### Message Handling

- Client generates request IDs sequentially
- Wraps requests in `Message::Request { id, request }`
- Server wraps responses in `Message::Response { id, response }`
- Client matches responses to pending requests by ID
- Server-sent messages (logs) come as `Message::Log` without IDs
- Client polls `receive_message()` and handles messages as they arrive
