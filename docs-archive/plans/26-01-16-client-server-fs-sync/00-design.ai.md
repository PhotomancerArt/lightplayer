# Design: Client-Server Filesystem Sync

## Overview

Enable `lp-client` and `lp-server` to communicate and perform filesystem operations. Clients can:
- List files recursively (or not recursively)
- Write a file
- Read a file
- Delete a file or directory

This design establishes the foundation for client-server communication. Project management (starting/stopping projects) will be handled later.

## File Structure

```
lp-shared/src/
├── output/
│   ├── mod.rs
│   ├── provider.rs          # OutputProvider trait, OutputChannelHandle, OutputFormat
│   └── memory.rs            # MemoryOutputProvider implementation
├── transport/
│   ├── mod.rs
│   ├── client.rs            # ClientTransport trait
│   └── server.rs            # ServerTransport trait
└── error.rs                 # Add OutputError

lp-model/src/server/
├── api.rs                   # ServerRequest, ServerResponse (project management)
├── fs_api.rs               # NEW: FsRequest, FsResponse (filesystem operations)
└── mod.rs

lp-model/src/message.rs     # NEW: Message envelope (ClientMessage, ServerMessage)

lp-server/src/
├── lib.rs
├── server.rs               # NEW: LpServer struct and tick() implementation
├── handlers.rs             # NEW: Message handlers (fs and project)
├── project.rs              # MODIFY: Project::new() takes OutputProvider
├── project_manager.rs
└── error.rs

lp-client/src/
├── lib.rs
├── client.rs               # NEW: LpClient struct
├── error.rs                # NEW: ClientError
└── transport/
    └── memory.rs           # NEW: In-memory transport for tests (with JSON serialization)

lp-shared/src/fs/
├── lp_fs.rs                # MODIFY: Add delete_file(), delete_dir(), list_dir(recursive)
├── lp_fs_std.rs            # MODIFY: Implement new methods
└── lp_fs_mem.rs            # MODIFY: Implement new methods (already has delete_file)
```

## Type Tree

### lp-shared/src/output/provider.rs
- `pub trait OutputProvider` - Moved from lp-engine, uses OutputError instead of lp-engine::Error
  - `fn open(&self, pin: u32, byte_count: u32, format: OutputFormat) -> Result<OutputChannelHandle, OutputError>`
  - `fn write(&self, handle: OutputChannelHandle, data: &[u8]) -> Result<(), OutputError>`
  - `fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError>`
- `pub struct OutputChannelHandle(i32)` - Handle for opened output channel
- `pub enum OutputFormat { Ws2811 }` - Output protocol format

### lp-shared/src/output/memory.rs
- `pub struct MemoryOutputProvider` - In-memory implementation for testing
  - Moved from lp-engine, uses OutputError

### lp-shared/src/transport/client.rs
- `pub trait ClientTransport` - Trait for client-side transport
  - `fn send(&mut self, msg: Message) -> Result<(), TransportError>` - Send message (consumes)
  - `fn receive(&mut self) -> Result<Option<Message>, TransportError>` - Receive message (non-blocking)

### lp-shared/src/transport/server.rs
- `pub trait ServerTransport` - Trait for server-side transport
  - `fn send(&mut self, msg: Message) -> Result<(), TransportError>` - Send message (consumes)
  - `fn receive(&mut self) -> Result<Option<Message>, TransportError>` - Receive message (non-blocking)

### lp-model/src/message.rs
- `pub enum Message` - Top-level message envelope
  - `Client(ClientMessage)` - Message from client
  - `Server(ServerMessage)` - Message from server
- `pub struct ClientMessage { id: u64, msg: ClientRequest }` - Client message with request ID
- `pub struct ServerMessage { id: u64, msg: ServerResponse }` - Server message with request ID
- `pub enum ClientRequest` - Client request types (filesystem + project management)
- `pub enum ServerResponse` - Server response types (filesystem + project management)

### lp-model/src/server/fs_api.rs
- `pub enum FsRequest` - Filesystem operation requests
  - `Read { path: String }`
  - `Write { path: String, data: Vec<u8> }`
  - `DeleteFile { path: String }`
  - `DeleteDir { path: String }`
  - `ListDir { path: String, recursive: bool }`
- `pub enum FsResponse` - Filesystem operation responses (all include error option)
  - `Read { path: String, data: Option<Vec<u8>>, error: Option<String> }`
  - `Write { path: String, error: Option<String> }`
  - `DeleteFile { path: String, error: Option<String> }`
  - `DeleteDir { path: String, error: Option<String> }`
  - `ListDir { path: String, entries: Vec<String>, error: Option<String> }`

### lp-model/src/server/api.rs
- `pub enum ServerRequest` - Server request types
  - `Filesystem(FsRequest)` - Wrapper for filesystem requests
  - `LoadProject { path: String }` - Project management requests
  - `UnloadProject { handle: ProjectHandle }`
  - `ListAvailableProjects`
  - `ListLoadedProjects`
  - `ProjectRequest { handle: ProjectHandle, request: ProjectRequest }`
- `pub enum ServerResponse` - Server response types
  - `Filesystem(FsResponse)` - Wrapper for filesystem responses
  - `LoadProject { handle: ProjectHandle }` - Project management responses
  - `UnloadProject`
  - `ListAvailableProjects { projects: Vec<AvailableProject> }`
  - `ListLoadedProjects { projects: Vec<LoadedProject> }`
  - `ProjectRequest { response: ProjectResponse }`

### lp-server/src/server.rs
- `pub struct LpServer` - Main server struct
  - `output_provider: Rc<RefCell<dyn OutputProvider>>` - Output provider (shared, mutable)
  - `project_manager: ProjectManager` - Manages loaded projects
  - `base_fs: Box<dyn LpFs>` - Server root filesystem (projects in `projects/` subdirectory)
- `impl LpServer`
  - `pub fn new(output_provider: Rc<RefCell<dyn OutputProvider>>, base_fs: Box<dyn LpFs>) -> Self`
  - `pub fn tick(&mut self, delta_ms: u32, incoming: Vec<Message>) -> Result<Vec<Message>, ServerError>` - Process messages and update state

### lp-server/src/handlers.rs
- Filesystem operation handlers (operate on `base_fs`)
- Project management handlers (operate on `project_manager`)

### lp-client/src/client.rs
- `pub struct LpClient<T: ClientTransport>` - Client struct
  - `transport: T` - Transport implementation
  - `next_request_id: u64` - Request ID counter
  - `pending_requests: HashMap<u64, PendingRequest>` - Track pending requests
- `impl<T: ClientTransport> LpClient<T>`
  - `pub fn new(transport: T) -> Self`
  - `pub fn fs_read(&mut self, path: String) -> Result<Vec<u8>, ClientError>` - Blocking, polls internally
  - `pub fn fs_write(&mut self, path: String, data: Vec<u8>) -> Result<(), ClientError>`
  - `pub fn fs_delete_file(&mut self, path: String) -> Result<(), ClientError>`
  - `pub fn fs_delete_dir(&mut self, path: String) -> Result<(), ClientError>`
  - `pub fn fs_list_dir(&mut self, path: String, recursive: bool) -> Result<Vec<String>, ClientError>`
  - `fn process_messages(&mut self) -> Result<(), ClientError>` - Internal: process incoming messages
  - `fn send_request(&mut self, request: ClientRequest) -> Result<ServerResponse, ClientError>` - Internal: send and wait for response

### lp-client/src/transport/memory.rs
- `pub struct MemoryTransport` - In-memory transport for tests
  - Uses channels (Sender/Receiver) for bidirectional communication
  - Implements both `ClientTransport` and `ServerTransport`
  - **Important**: Serializes/deserializes messages to/from JSON to ensure serialization works correctly

### lp-shared/src/fs/lp_fs.rs
- `pub trait LpFs` - Filesystem trait (modified)
  - `fn read_file(&self, path: &str) -> Result<Vec<u8>, FsError>`
  - `fn write_file(&self, path: &str, data: &[u8]) -> Result<(), FsError>`
  - `fn file_exists(&self, path: &str) -> Result<bool, FsError>`
  - `fn list_dir(&self, path: &str, recursive: bool) -> Result<Vec<String>, FsError>` - **MODIFIED**: Added recursive parameter
  - `fn delete_file(&self, path: &str) -> Result<(), FsError>` - **NEW**
  - `fn delete_dir(&self, path: &str) -> Result<(), FsError>` - **NEW**: Always recursive
  - `fn chroot(&self, subdir: &str) -> Result<Box<dyn LpFs>, FsError>`

## Key Design Decisions

### Message Protocol
- **Message Envelope**: All messages wrapped in `Message` enum with request IDs for correlation
- **Wrapper Pattern**: Filesystem messages in separate `fs_api.rs` module, wrapped in `ServerRequest::Filesystem(FsRequest)`
- **Error Handling**: Errors included in response variants (e.g., `data: Option<Vec<u8>>, error: Option<String>`)
- **JSON Serialization**: All messages serializable via serde_json

### Transport
- **Sync Polling**: Simple sync polling interface (no async runtime dependency)
- **Separate Traits**: `ClientTransport` and `ServerTransport` traits in `lp-shared`
- **Message Consumption**: Messages consumed (moved) on send
- **Non-blocking Receive**: Returns `Option<Message>` for non-blocking receive

### Filesystem Operations
- **Delete Operations**: Separate `delete_file()` and `delete_dir()` methods (delete_dir always recursive)
- **Recursive Listing**: `list_dir()` takes `recursive: bool` parameter
- **Security**: Path validation prevents deleting outside root directory, explicitly rejects "/"
- **Server Scope**: LpServer operates on server root filesystem (projects in `projects/` subdirectory)

### Server Architecture
- **Tick-based API**: `tick(delta_ms, incoming_messages) -> outgoing_messages` - game-engine style
- **OutputProvider**: `Rc<RefCell<dyn OutputProvider>>` for `no_std` compatibility and interior mutability
- **Project Integration**: Projects receive OutputProvider from LpServer (not created internally)

### Client Architecture
- **Blocking API**: All operations are blocking, poll internally until response received
- **Single-threaded**: No async complexity (important for WebAssembly)
- **Request Correlation**: Tracks pending requests by ID, matches responses

### Testing
- **In-memory Transport**: Uses channels, serializes/deserializes to/from JSON to verify serialization works
- **Path Validation Testing**: Extract validation logic to separate function, test that (don't attempt dangerous operations)

## Implementation Notes

### OutputProvider Move
- Move `OutputProvider` trait and `MemoryOutputProvider` from `lp-engine` to `lp-shared`
- Create `OutputError` in `lp-shared/src/error.rs`
- Update `lp-engine` to import from `lp-shared`
- Update `lp-server` to import from `lp-shared` (currently imports from `lp-engine`)

### Message API Consolidation
- Remove `lp-api` crate (old code)
- Add filesystem messages to `lp-model/src/server/fs_api.rs`
- Update `ServerRequest`/`ServerResponse` to include `Filesystem` wrapper variants
- Create message envelope types in `lp-model/src/message.rs`

### Filesystem Trait Updates
- Add `delete_file()` and `delete_dir()` to `LpFs` trait
- Update `list_dir()` signature to include `recursive: bool`
- Implement in `LpFsStd` and `LpFsMemory`
- Ensure path validation prevents dangerous operations (especially for `LpFsStd`)

### LpServer Implementation
- Create `LpServer` struct with tick-based API
- Implement message handlers for filesystem and project operations
- Update `Project::new()` to take `OutputProvider` parameter

### LpClient Implementation
- Create `LpClient` struct with blocking filesystem operations
- Implement request/response correlation
- Create in-memory transport with JSON serialization for tests
