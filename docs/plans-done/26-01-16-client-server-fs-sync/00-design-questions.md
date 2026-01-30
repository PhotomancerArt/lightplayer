# Design Questions: Client-Server Filesystem Sync

## Scope

Enable `lp-client` and `lp-server` to communicate and perform filesystem operations. Clients should be able to:
- List files recursively (or not recursively)
- Write a file
- Read a file
- Delete a file or directory

We'll handle project management (starting/stopping projects) later. Focus is on basic filesystem sync operations.

## Current State

- **Message Protocol (`lp-api`)**: Has `ClientMsg` and `ServerMsg` enums with basic FS operations (`FsRead`, `FsWrite`, `FsExists`, `FsListDir`), but missing delete operations and recursive listing option. **NOTE: `lp-api` is old code and should be removed - messages should be in `lp-model`**
- **Message Protocol (`lp-model`)**: Has `ServerRequest` and `ServerResponse` in `lp-model/src/server/api.rs`, but focused on project management (LoadProject, UnloadProject, etc.)
- **Filesystem Trait (`lp-shared`)**: `LpFs` trait has `read_file`, `write_file`, `file_exists`, `list_dir`, `chroot`. Missing delete operations (though `LpFsMemory` has `delete_file()` not in trait)
- **OutputProvider (`lp-engine`)**: Currently in `lp-engine/src/output/provider.rs` - should move to `lp-shared` since used by both `lp-server` and `lp-engine`
- **Server (`lp-server`)**: Has `ProjectManager` and `Project` structures, but no `LpServer` struct, no message handlers, no transport layer
- **Client (`lp-client`)**: Just a stub, no `LpClient` struct, no implementation
- **Transport**: No transport abstraction exists yet

## Expanded Scope

1. **Filesystem operations at LpServer level**: Filesystem messages should occur at the LpServer level, not within a project. The server manages filesystem operations directly (probably on the projects base directory).
2. **LpServer concept**: Need an `LpServer` struct - the main server that connects with transport, takes OutputProvider
3. **OutputProvider in lp-shared**: Move OutputProvider from `lp-engine` to `lp-shared` (used by both `lp-server` and `lp-engine`)
4. **Message API in lp-model**: Move/consolidate message API to `lp-model` (remove `lp-api` crate)
5. **LpClient struct**: `lp-client` needs `LpClient` struct for client-side operations (filesystem and eventually project management)

## Questions

### Question 1: Request/Response Correlation

**Current State:**
- The existing `lp-api` messages (`ClientMsg`/`ServerMsg`) don't have request IDs
- Previous plans (in `lp-glsl/plans/26-01-08-lp-client-server-sync`) discussed using request IDs with message envelopes for correlation
- For filesystem operations, we need to match responses to requests

**Question:**
How should we handle request/response correlation for filesystem operations?

**Options:**
- **Option A**: Add request IDs to each message variant (e.g., `FsRead { id: u64, path: String }`)
- **Option B**: Use a message envelope wrapper (e.g., `Message { id: u64, payload: ClientMsg }`)
- **Option C**: Keep it simple for now - assume synchronous request/response (one request at a time, wait for response)

**Suggested Course Forward:**
I recommend **Option B** (message envelope) because:
- Allows multiple requests in flight (async operations)
- Keeps message types clean (no ID fields in every variant)
- Matches the design from previous plans
- More flexible for future needs

However, if we want to keep it simple initially, **Option C** could work for basic filesystem sync.

**DECIDED: Option B - Message envelope wrapper**

**Decision:**
- Use message envelope wrapper (e.g., `Message { id: u64, payload: ClientMsg }`)
- Enables concurrent async messages (needed for log messages and other server-sent messages)
- Keeps message types clean
- Request IDs are client-generated u64 values

---

### Question 2: Transport Trait Design

**Current State:**
- No transport abstraction exists yet
- Need to support multiple transport types:
  - In-memory queue (for tests and same-process scenarios)
  - Serial (desktop OS, WebSerial, ESP32 HAL)
  - WebSockets (most likely)
- User specified: transports should take and produce immutable message objects (consuming them)

**Question:**
How should we design the transport trait? Key considerations:
- Where should the trait live? (`lp-api`, `lp-shared`, or separate crate?)
- Should it be bidirectional (client and server use same trait) or separate traits?
- How should messages be consumed/produced? (move semantics)
- Should it be async or sync? (polling vs async/await)

**Options:**

**Option A: Simple sync trait in `lp-api`**
```rust
pub trait Transport {
    fn send(&mut self, message: Message) -> Result<(), TransportError>;
    fn receive(&mut self) -> Result<Option<Message>, TransportError>;
}
```
- Simple polling-based interface
- Messages consumed (moved) on send
- Returns `Option<Message>` for non-blocking receive
- Works well for in-memory and serial transports

**Option B: Async trait**
```rust
#[async_trait]
pub trait Transport {
    async fn send(&mut self, message: Message) -> Result<(), TransportError>;
    async fn receive(&mut self) -> Result<Message, TransportError>;
}
```
- Better for WebSocket transports
- Requires async runtime
- May be overkill for serial/in-memory

**Option C: Separate client/server traits**
- `ClientTransport` for client-side (one connection)
- `ServerTransport` for server-side (multiple connections)
- More complex but matches different use cases

**Suggested Course Forward:**
I recommend **Option A** (simple sync trait in `lp-api`) because:
- Simple polling interface works for all transport types
- In-memory and serial transports are naturally sync/polling
- WebSocket can buffer messages and implement polling interface
- No async runtime dependency (important for `no_std` environments like ESP32)
- Messages are consumed (moved) as specified
- Can add async wrapper later if needed

The trait should live in `lp-api` since it's part of the client-server protocol.

**DECIDED: Simple sync polling trait with separate client/server traits, in `lp-shared`**

**Decision:**
- Use simple sync polling trait (no async runtime dependency, important for embedded/ESP32)
- Separate `ClientTransport` and `ServerTransport` traits (different use cases: one connection vs multiple)
- Trait lives in `lp-shared` (shared infrastructure, not protocol-specific)
- Messages consumed (moved) on send: `fn send(&mut self, message: Message) -> Result<(), TransportError>`
- Non-blocking receive: `fn receive(&mut self) -> Result<Option<Message>, TransportError>`
- Can expand to async later if needed

---

### Question 3: Delete Operations

**Current State:**
- `LpFs` trait doesn't have delete operations
- `LpFsMemory` has `delete_file()` method but it's not part of the trait
- Need to support deleting both files and directories
- User requirement: "delete a file or directory"

**Question:**
How should delete operations work?

**Options:**

**Option A: Single `delete` method that handles both files and directories**
```rust
fn delete(&self, path: &str) -> Result<(), FsError>;
```
- Automatically detects if path is file or directory
- For directories: recursive by default (deletes all contents)
- Simple API, matches common filesystem behavior

**Option B: Separate `delete_file` and `delete_dir` methods**
```rust
fn delete_file(&self, path: &str) -> Result<(), FsError>;
fn delete_dir(&self, path: &str, recursive: bool) -> Result<(), FsError>;
```
- More explicit, clearer intent
- Can control recursive behavior for directories
- More methods to implement

**Option C: Single `delete` with recursive parameter**
```rust
fn delete(&self, path: &str, recursive: bool) -> Result<(), FsError>;
```
- Single method but explicit about recursive behavior
- `recursive` parameter ignored for files
- Middle ground between A and B

**Suggested Course Forward:**
I recommend **Option A** (single `delete` method, recursive by default) because:
- Matches common filesystem behavior (rm -rf, most file managers)
- Simpler API - one method instead of two
- Recursive is usually what you want (safer to require explicit check for non-empty)
- Can add separate methods later if needed

However, **Option B** might be clearer and safer (explicit about recursive deletion).

**DECIDED: Option B - Separate delete_file and delete_dir, delete_dir always recursive**

**Decision:**
- Separate `delete_file(&self, path: &str) -> Result<(), FsError>` and `delete_dir(&self, path: &str) -> Result<(), FsError>` methods
- Clear intent - explicit about what you're deleting
- `delete_dir` is always recursive (no parameter needed, avoids that class of errors)
- More methods to implement but clearer and safer API

**CRITICAL SECURITY REQUIREMENT:**
- **MUST** use existing path validation from `resolve_and_validate()` / `get_path()` methods
- **MUST NOT** allow deleting outside the root directory (especially for `LpFsStd`)
- **MUST NOT** allow deleting "/" (root) - should explicitly check and return error before any filesystem operation
- Note: Existing `resolve_and_validate()` normalizes "/" to empty string, which would resolve to root directory - delete methods must explicitly reject "/" first
- **Testing approach**: Extract path validation logic into a separate function and test *that* - do NOT attempt to delete "/" or paths outside root in tests
- Path validation must happen before any delete operation
- Consider adding `FsError::InvalidPath` variant for root deletion attempts

---

### Question 4: Recursive Listing

**Current State:**
- `LpFs` trait has `list_dir(&self, path: &str)` which lists only immediate children (non-recursive)
- User requirement: "list files recursively (or not recursively)"
- Need to support both recursive and non-recursive listing

**Question:**
How should we add recursive listing support?

**Options:**

**Option A: Add `recursive` parameter to existing `list_dir`**
```rust
fn list_dir(&self, path: &str, recursive: bool) -> Result<Vec<String>, FsError>;
```
- Single method, explicit parameter
- Backward compatible if we make `recursive` default to `false` (but trait methods can't have defaults)
- Breaking change to existing code

**Option B: Add separate `list_dir_recursive` method**
```rust
fn list_dir(&self, path: &str) -> Result<Vec<String>, FsError>;  // existing, non-recursive
fn list_dir_recursive(&self, path: &str) -> Result<Vec<String>, FsError>;  // new, recursive
```
- Non-breaking change
- Clear intent from method name
- Two methods to implement

**Option C: Replace `list_dir` with parameterized version**
```rust
fn list_dir(&self, path: &str, recursive: bool) -> Result<Vec<String>, FsError>;
```
- Single method, cleaner API
- Breaking change - need to update all callers
- More flexible

**Suggested Course Forward:**
I recommend **Option B** (separate `list_dir_recursive` method) because:
- Non-breaking change (existing code continues to work)
- Clear intent from method name (no need to check parameter)
- Matches pattern we're using for delete operations (separate methods)
- Can deprecate and consolidate later if desired

However, **Option C** might be cleaner long-term if we're okay with breaking changes.

**DECIDED: Option C - Add `recursive` parameter to `list_dir`**

**Decision:**
- Replace `list_dir(&self, path: &str)` with `list_dir(&self, path: &str, recursive: bool) -> Result<Vec<String>, FsError>`
- Single method, cleaner API
- Breaking change - need to update all callers (user doesn't care about backwards compat)
- More flexible and consistent API

---

### Question 5: Message Protocol Updates

**Current State:**
- `ClientMsg` has `FsRead`, `FsWrite`, `FsExists`, `FsListDir` (missing delete operations, missing recursive parameter)
- `ServerMsg` has corresponding responses (missing delete responses)
- No message envelope wrapper yet (decided in Question 1)
- `FsListDir` doesn't have `recursive` parameter

**Question:**
What changes do we need to make to the message protocol?

**Required Changes:**

1. **Message Envelope** (from Question 1):
   - Create `Message` enum wrapping `ClientMsg`/`ServerMsg` with request IDs
   - Structure: `Message { id: u64, payload: ClientMsg }` or `Message { id: u64, payload: ServerMsg }`
   - Or separate enums: `ClientMessage { id: u64, msg: ClientMsg }` and `ServerMessage { id: u64, msg: ServerMsg }`

2. **Delete Operations**:
   - Add `FsDeleteFile { path: String }` to `ClientMsg`
   - Add `FsDeleteDir { path: String }` to `ClientMsg`
   - Add `FsDeleteFileResponse { path: String, success: bool }` to `ServerMsg`
   - Add `FsDeleteDirResponse { path: String, success: bool }` to `ServerMsg`
   - Or use single `FsDeleteResponse` with a field indicating file vs dir?

3. **Recursive Listing**:
   - Update `FsListDir { path: String }` to `FsListDir { path: String, recursive: bool }`
   - `FsListDirResponse` already has `entries: Vec<String>` which should work for both

4. **Error Handling**:
   - Current `ServerMsg::Error { message: String }` is generic
   - Should we include error details in response variants (e.g., `FsReadResponse { path, data?, error? }`)?
   - Or keep separate error message?

**Suggested Course Forward:**
- **Message Envelope**: Use single `Message` enum with `Client(ClientMessage)` and `Server(ServerMessage)` variants, where `ClientMessage` and `ServerMessage` wrap the actual messages with IDs
- **Delete Operations**: Separate `FsDeleteFile` and `FsDeleteDir` messages and responses (matches filesystem trait design)
- **Error Handling**: Include error in response variants (e.g., `FsReadResponse { path, data: Option<Vec<u8>>, error: Option<String> }`) - allows success and error in same variant

**DECIDED: Yes - Use suggested approach**

**Decision:**
- **Message Envelope**: Single `Message` enum with `Client(ClientMessage)` and `Server(ServerMessage)` variants
- **Delete Operations**: Separate `FsDeleteFile` and `FsDeleteDir` messages and responses
- **Recursive Listing**: Update `FsListDir` to include `recursive: bool` parameter
- **Error Handling**: Include error in response variants (e.g., `FsReadResponse { path, data: Option<Vec<u8>>, error: Option<String> }`)

---

### Question 6: LpServer Structure

**Current State:**
- `lp-server` has `ProjectManager` and `Project` but no main `LpServer` struct
- No connection to transport layer
- `Project::new()` currently creates its own `MemoryOutputProvider` - should come from LpServer

**Question:**
What should the `LpServer` struct look like?

**Key Considerations:**
- Needs to hold `OutputProvider` (Arc<dyn OutputProvider>)
- Needs to hold `ProjectManager` (or should ProjectManager be part of LpServer?)
- Needs to handle transport (but transport is pluggable - how do we connect it?)
- Needs to handle filesystem operations (what filesystem does it operate on?)
- Needs to process messages from transport

**Options:**

**Option A: LpServer owns everything**
```rust
pub struct LpServer {
    output_provider: Arc<dyn OutputProvider>,
    project_manager: ProjectManager,
    base_fs: Box<dyn LpFs>,  // Filesystem for projects base directory
}
```
- LpServer owns ProjectManager
- LpServer has base filesystem for filesystem operations
- Transport passed to `handle_message()` method

**Option B: LpServer is a handler, doesn't own transport**
```rust
pub struct LpServer {
    output_provider: Rc<RefCell<dyn OutputProvider>>,  // Note: Rc<RefCell> for no_std
    project_manager: ProjectManager,
    base_fs: Box<dyn LpFs>,
}

impl LpServer {
    pub fn handle_message(&mut self, msg: Message) -> Result<Message, Error>;
}
```
- LpServer processes messages but doesn't own transport
- App/firmware owns transport and calls handle_message
- Clean separation

**Option C: Tick-based API**
```rust
pub struct LpServer {
    output_provider: Rc<RefCell<dyn OutputProvider>>,
    project_manager: ProjectManager,
    base_fs: Box<dyn LpFs>,
}

impl LpServer {
    pub fn tick(&mut self, delta_ms: u32, incoming: Vec<Message>) -> Result<Vec<Message>, Error>;
}
```
- Game-loop style API
- Takes batch of incoming messages, returns batch of outgoing messages
- Can update internal state (projects, etc.) during tick
- More flexible for embedded/game-like systems

**Option D: Process messages method**
```rust
impl LpServer {
    pub fn handle_messages(&mut self, messages: Vec<Message>) -> Result<Vec<Message>, Error>;
}
```
- Process multiple messages at once
- Returns multiple responses
- Simpler than tick (no delta_ms)

**Suggested Course Forward:**
I recommend **Option B** initially, but **Option C** (tick-based) might be better because:
- Matches game-loop pattern (common in embedded systems)
- Allows server to update state (projects, etc.) during tick
- Batch processing is more efficient
- `delta_ms` could be useful for project runtime updates later

However, **Option D** is simpler if we don't need delta_ms.

**Note**: `Arc<dyn OutputProvider>` should be `Rc<RefCell<dyn OutputProvider>>` for `no_std` compatibility and interior mutability.

**DECIDED: Option C - Tick-based API**

**Decision:**
- Use `tick(&mut self, delta_ms: u32, incoming: Vec<Message>) -> Result<Vec<Message>, Error>`
- Matches game-engine pattern (graphics engine, maybe sound engine too)
- Allows server to update state during tick
- Batch processing is more efficient
- `delta_ms` useful for project runtime updates
- `OutputProvider` is `Rc<RefCell<dyn OutputProvider>>` for `no_std` compatibility

---

### Question 7: Filesystem Scope for LpServer

**Current State:**
- `ProjectManager` has `projects_base_dir` - base directory where projects are stored
- Each project has its own filesystem (chrooted to project directory)
- Filesystem operations need to happen at LpServer level

**Question:**
What filesystem scope should LpServer operate on?

**Options:**

**Option A: Projects base directory**
- LpServer's filesystem is rooted at projects base directory
- Filesystem operations can access any project's files
- Paths like `/project-name/file.txt` or `/project-name/src/shader.glsl`
- Matches current ProjectManager structure

**Option B: Root filesystem (entire server filesystem)**
- LpServer's filesystem is the root filesystem
- Can access projects directory and other server files
- More flexible but potentially less secure
- Paths like `/projects/project-name/file.txt`

**Option C: Separate filesystem per operation**
- LpServer doesn't have a single filesystem
- Each operation specifies which filesystem to use
- More complex but more flexible

**Suggested Course Forward:**
I recommend **Option A** (projects base directory) because:
- Matches current ProjectManager structure
- Clear scope - server manages projects
- Security boundary - can't access outside projects directory
- Simple path structure

**DECIDED: Option B variant - LpServer has its own root, projects in `projects/` subdirectory**

**Decision:**
- LpServer's filesystem root is the server root directory
- Projects are kept in `projects/` subdirectory
- Future `server.json` will be at the root (`/server.json`)
- Filesystem operations can access both server config and project files
- Paths: `/projects/project-name/file.txt` for project files, `/server.json` for server config
- Clear separation: server config at root, projects in subdirectory
- ProjectManager's `projects_base_dir` would be `projects/` (relative to server root)

---

### Question 8: OutputProvider Move to lp-shared

**Current State:**
- `OutputProvider` trait is in `lp-engine/src/output/provider.rs`
- `MemoryOutputProvider` is in `lp-engine/src/output/memory.rs`
- `lp-server` currently creates its own `MemoryOutputProvider` in `Project::new()`
- Both `lp-server` and `lp-engine` need OutputProvider

**Question:**
How should we move OutputProvider to lp-shared?

**Considerations:**
- OutputProvider trait currently uses `lp-engine::Error` - need to decide on error type
- MemoryOutputProvider uses `alloc` (which lp-shared already has)
- `lp-engine` already depends on `lp-shared`
- `lp-server` already depends on `lp-shared`

**Error Type Options:**
- **Option A**: Create new `OutputError` in `lp-shared` (simple, specific)
- **Option B**: Use generic error type (more flexible but more complex)
- **Option C**: Use `lp-shared::FsError` (but OutputProvider isn't filesystem-specific)

**Suggested Course Forward:**
Create new `OutputError` in `lp-shared` - simple and specific to output operations. Can convert to/from other error types as needed.

**Tasks:**
1. Move `OutputProvider` trait from `lp-engine/src/output/provider.rs` to `lp-shared/src/output/provider.rs`
2. Move `OutputChannelHandle` and `OutputFormat` types as well
3. Move `MemoryOutputProvider` from `lp-engine/src/output/memory.rs` to `lp-shared/src/output/memory.rs`
4. Update `lp-engine` to import from `lp-shared`
5. Update `lp-server` to import from `lp-shared` (currently imports from `lp-engine`)
6. Update `Project::new()` to take `Arc<dyn OutputProvider>` instead of creating its own

**Suggested Course Forward:**
This is straightforward - just move the files and update imports. No architectural questions, just implementation.

---

### Question 9: Message API Consolidation

**Current State:**
- `lp-api` crate has `ClientMsg` and `ServerMsg` enums (old code, should be removed)
- `lp-model/src/server/api.rs` has `ServerRequest` and `ServerResponse` enums (focused on project management)
- Need to consolidate filesystem messages into `lp-model`

**Question:**
How should we consolidate the message API?

**Options:**

**Option A: Extend existing ServerRequest/ServerResponse**
- Add filesystem operations to `ServerRequest` enum in `lp-model/src/server/api.rs`
- Add filesystem responses to `ServerResponse` enum
- Keep everything in one place
- But ServerRequest/ServerResponse might get large

**Option B: Separate filesystem messages**
- Create `FsRequest` and `FsResponse` enums in `lp-model/src/server/api.rs`
- `ServerRequest` has variant: `Filesystem(FsRequest)`
- `ServerResponse` has variant: `Filesystem(FsResponse)`
- More organized, but adds nesting

**Option C: Separate module for filesystem messages**
- Create `lp-model/src/server/fs_api.rs` with `FsRequest` and `FsResponse`
- Keep project management in `api.rs`
- Both used by message envelope
- Uses wrapper pattern: `ServerRequest::Filesystem(FsRequest)`

**Note on Rust enum limitations:**
- Rust doesn't allow splitting enum variants across files
- Must use composition (wrapper pattern) to organize
- Serde can serialize nested enums to JSON reasonably (e.g., `{"Filesystem": {"Read": {"path": "/file.txt"}}}`)
- In-memory transport for tests should serialize/deserialize to/from JSON to ensure it works correctly

**Suggested Course Forward:**
I recommend **Option A** (extend existing ServerRequest/ServerResponse) because:
- Simpler - all server messages in one place
- Filesystem operations are server operations
- Can organize with comments/sections
- Matches current structure

However, if we expect many message types, **Option C** might be better for organization.

**DECIDED: Option C - Separate module with wrapper pattern**

**Decision:**
- Create `lp-model/src/server/fs_api.rs` with `FsRequest` and `FsResponse` enums
- `ServerRequest` has variant: `Filesystem(FsRequest)`
- `ServerResponse` has variant: `Filesystem(FsResponse)`
- Uses wrapper pattern (composition) since Rust doesn't allow splitting enum variants across files
- Serde can serialize nested enums to JSON reasonably
- **Important**: In-memory transport for tests must serialize/deserialize to/from JSON to ensure serialization works correctly

---

### Question 10: LpClient Structure

**Current State:**
- `lp-client` is just a stub with a dummy `add` function
- Need `LpClient` struct for client-side operations
- Client needs to handle filesystem operations and eventually project management

**Question:**
What should the `LpClient` struct look like?

**Key Considerations:**
- Needs to hold transport (generic over transport trait)
- Needs to handle request/response correlation (request IDs)
- Needs to handle filesystem operations
- Eventually needs to handle project management (but not now)
- Needs to process incoming messages (polling)

**Suggested Structure:**
```rust
pub struct LpClient<T: ClientTransport> {
    transport: T,
    next_request_id: u64,
    pending_requests: HashMap<u64, PendingRequest>,
}

impl<T: ClientTransport> LpClient<T> {
    pub fn new(transport: T) -> Self;
    
    // Filesystem operations
    pub fn fs_read(&mut self, path: String) -> Result<Vec<u8>, ClientError>;
    pub fn fs_write(&mut self, path: String, data: Vec<u8>) -> Result<(), ClientError>;
    pub fn fs_delete_file(&mut self, path: String) -> Result<(), ClientError>;
    pub fn fs_delete_dir(&mut self, path: String) -> Result<(), ClientError>;
    pub fn fs_list_dir(&mut self, path: String, recursive: bool) -> Result<Vec<String>, ClientError>;
    
    // Internal: process incoming messages
    fn process_messages(&mut self) -> Result<(), ClientError>;
    
    // Internal: send request and wait for response
    fn send_request(&mut self, request: ClientMessage) -> Result<ServerMessage, ClientError>;
}
```

**Questions:**
- Should `fs_read` etc. be async (polling) or blocking?
- How do we handle timeouts?
- Should we have a `poll()` method that processes messages, or do operations automatically poll?

**Suggested Course Forward:**
- Operations are blocking (they poll internally until response received)
- Simple timeout mechanism (max iterations or time-based if std available)
- `process_messages()` called internally by operations
- Can add async/polling API later if needed

**DECIDED: Blocking, single-threaded API**

**Decision:**
- Everything is blocking and single-threaded
- `lp-client` will likely be run in WebAssembly - want to avoid async complexity for now
- Operations poll internally until response received (blocking)
- Simple timeout mechanism (max iterations or time-based if std available)
- `process_messages()` called internally by operations
- Can revisit async in the future if needed
- Project management functions can be added later
- Structure looks good as suggested

**Note**: Since filesystem operations are at LpServer level (operating on server root filesystem, not project-specific), we don't need project context in filesystem messages. This simplifies the design.
