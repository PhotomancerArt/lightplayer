# Questions: LP Client-Server Sync

## Goal

Create an `lp-client` library that can sync with an `lp-server` instance, enabling:
- Client-server communication for project management and node state synchronization
- Frame-based incremental updates to minimize bandwidth
- Support for multiple transport mechanisms (websockets, serial) with feature gating
- Test infrastructure to verify client-server sync works correctly

## Current State

**Existing infrastructure:**
- `lp-shared/src/project/api.rs`: Defines `ProjectRequest` and `ProjectResponse` with frame-based change tracking
- `lp-shared/src/server/api.rs`: Defines `ServerRequest` and `ServerResponse` for project management
- `lp-shared/src/project/handle.rs`: `ProjectHandle` (i32) for runtime project identification
- `lp-shared/src/project/nodes/handle.rs`: `NodeHandle` (i32) for runtime node identification
- `lp-shared/src/project/frame_id.rs`: `FrameId` (i64) for versioning changes
- `lp-shared/src/project/nodes/state.rs`: `NodeState` enum with `OutputState`, `ShaderState`, `TextureState`
- `lp-engine`: Contains `ProjectRuntime` that manages node runtimes
- `lp-server`: Basic project management exists
- `lp-client`: Empty crate with placeholder code

**Architectural decisions made:**
- Using integer handles (i32) for runtime identification instead of paths
- Frame-based versioning similar to Subversion
- Two-level API: server-level (project management) and project-level (node state)
- Node state includes detail (texture pixels, shader errors, output values)

**Open questions:**
- Handle-to-path mapping mechanism
- Frame ID lifecycle and tracking
- Change granularity and tracking
- Texture serialization strategy
- Memory tracking details
- Transport abstraction design

## Questions

### Question 1: Handle-to-Path Mapping

**Current State:**
- `ProjectRuntime` currently uses path-based IDs (`TextureId`, `ShaderId`, `OutputId`, `FixtureId`) which are `String` types containing paths like "/src/my-shader.shader"
- `NodeHandle` and `ProjectHandle` are defined as `i32` wrappers but not yet integrated into the runtime
- The API in `lp-shared/src/project/api.rs` uses `NodeHandle` in responses, but the engine doesn't yet assign or track handles
- `NodeDetail` includes a `path: String` field, suggesting handles and paths coexist

**Question:**
How should we map between runtime handles (i32) and config paths (String)? We need this mapping for:
- Server: Converting engine's path-based nodes to handle-based API responses
- Client: Understanding which handle corresponds to which node path
- Both: Handling node creation/deletion and maintaining consistency

**Suggested Options:**

**Option A: Handle Registry in ProjectRuntime**
- Add a `HandleRegistry` to `ProjectRuntime` that maintains bidirectional mapping: `handle → path` and `path → handle`
- Assign handles sequentially (0, 1, 2...) when nodes are initialized
- Store handles in node runtimes (add `handle: NodeHandle` field)
- When responding to `GetChanges`, convert path-based nodes to handle-based using registry

**Option B: Handle Registry in lp-server**
- Keep `ProjectRuntime` path-based internally
- `lp-server` maintains handle registry per project
- Server translates between handles and paths when communicating with clients
- Engine remains unchanged, server handles translation

**Option C: Hybrid - Handles in Runtime, Registry in Server**
- `ProjectRuntime` assigns and stores handles in node runtimes
- `lp-server` maintains its own registry for fast lookups and client queries
- Both maintain the mapping, server's is authoritative for API

**DECIDED: Option A - Handle Registry in ProjectRuntime**

**Decision:**
- **Update node HashMaps to use handles as keys** instead of path-based IDs (`HashMap<NodeHandle, ...>`)
- Node runtimes store their own path (for debugging/config access)
- **No separate registry needed** - the HashMap itself IS the registry (handle → node → path)
- **Just need an incrementing i32 counter** for the next handle ID (`next_handle: i32`)
- Node runtimes should store handles to nodes they reference (e.g., fixture stores `NodeHandle` for its output, not path)
- Handles assigned sequentially during initialization (0, 1, 2...)
- Handles are runtime-specific (not stable across reloads)
- To look up handle from path: iterate HashMap (acceptable since this is rare - only needed for file change handling)

---

### Question 2: Frame ID Lifecycle and Tracking

**Current State:**
- `FrameId` is defined as `i64` wrapper in `lp-shared/src/project/frame_id.rs`
- `ProjectRequest::GetChanges` includes `since_frame: FrameId` parameter
- `ProjectResponse::GetChanges` includes `current_frame: FrameId`
- `ProjectRuntime` has `frame_time: FrameTime` which tracks `total_ms` and `delta_ms` but no frame counter
- No frame tracking infrastructure exists yet

**Question:**
How should frame IDs be managed? We need to:
- Track the current frame ID per project
- Know which frame each node was created/modified in
- Efficiently determine which nodes changed since a given frame
- Handle frame increments (every frame? only on changes?)

**Clarification:**
We need to track different types of changes:
- **Config changes**: Rare (when node.json or GLSL files change), but we need to track `last_config_frame` per node (not used yet, but will be)
- **State changes**: Often update every frame (texture pixels, output values, shader errors), but not always. This is what `GetChanges` uses via `last_state_frame`

**Decision:**
- Increment `FrameId` every `ProjectRuntime::update()` call (frame counter for timing/synchronization)
- Per-node tracking:
  - `created_frame: FrameId` - when node was created
  - `last_config_frame: FrameId` - when config last changed (for future use)
  - `last_state_frame: FrameId` - when state last changed (used for `GetChanges` responses)
- Frame IDs are per-project
- Frame IDs reset to 0 on project reload (fresh start)

**What constitutes a change:**
- **Config change**: Node config file modified (node.json, main.glsl, etc.) → update `last_config_frame`
- **State change**: Node runtime state changes (texture pixels updated, shader error occurs, output values change) → update `last_state_frame`
- **Creation**: Node initialized → set `created_frame` and initial `last_config_frame`/`last_state_frame`

---

### Question 3: Texture Serialization Strategy

**Current State:**
- `TextureState` contains a full `Texture` struct with pixel data (`Vec<u8>`)
- `Texture` can be large (e.g., 64x64 RGB8 = 12KB, 256x256 RGBA8 = 256KB)
- `NodeDetail` includes full `NodeState` which for textures includes all pixel data
- `GetChanges` response includes `node_detail: HashMap<NodeHandle, NodeDetail>` for matching nodes
- User mentioned concern about memory usage with serialization

**Question:**
How should we handle texture data in sync responses? We need to balance:
- Bandwidth efficiency (don't send unchanged texture data)
- Memory usage (avoid excessive copying)
- Client needs (when does client actually need texture pixels?)

**Suggested Options:**

**Option A: Always include full texture data**
- Include complete pixel data in `TextureState` whenever texture detail is requested
- Simple, but wasteful for large textures that haven't changed
- High memory/bandwidth cost

**Option B: Delta encoding / change detection**
- Only include texture data if pixels actually changed since `since_frame`
- Compare current texture data to previous frame's data
- More complex, requires storing previous frame's data
- Still sends full texture if any pixel changed

**Option C: Optional texture detail with explicit request**
- `NodeSpecifier` already supports requesting detail for specific nodes
- Only include texture pixel data when explicitly requested via `detail_specifier`
- For incremental updates, only send texture metadata (dimensions, format) unless requested
- Client can request full detail for specific textures it's displaying

**Option D: Compression for texture data**
- Always send texture data but compress it (e.g., zlib, or format-specific compression)
- Reduces bandwidth but adds CPU overhead
- Still sends data even if unchanged

**DECIDED: Always send full texture data, design for future compression**

**Decision:**
- Always include full texture pixel data in `TextureState` when texture detail is requested
- **Do NOT compress initially** (for simplicity)
- **Design state objects to allow compression later** (make data structure flexible enough to add compression without breaking changes)
- Compression (e.g., zlib) can be added later if program space allows
- Texture metadata (width, height, format) always included with pixel data

---

### Question 4: Transport Abstraction Design

**Current State:**
- `lp-engine/src/traits/transport.rs` defines a `Transport` trait for JSON messages (used by firmware)
- `lp-core-cli/src/transport.rs` implements `HostTransport` using stdio
- User mentioned needing websockets and serial transports with feature gating
- `lp-shared/src/fs/` has good examples of feature gating (with `std` feature)
- `lp-client` should separate transport cleanly so it can be gated with features

**Question:**
How should we design the transport abstraction for `lp-client`? We need:
- Support for websockets (server-side, for web clients)
- Support for serial (client-side, for ESP32 firmware)
- Feature gating to avoid pulling in unnecessary dependencies
- Clean separation so transport implementations can be in different crates

**Suggested Options:**

**Option A: Transport trait in lp-shared**
- Define a `ClientTransport` trait in `lp-shared` (similar to existing `Transport` trait)
- `lp-client` depends on `lp-shared` and uses the trait
- Transport implementations in separate crates:
  - `lp-client-websocket` (server-side, depends on websocket libs)
  - `lp-client-serial` (client-side, depends on serial libs, but will be in ESP32 firmware)
- `lp-client` core is transport-agnostic

**Option B: Transport trait in lp-client with feature gates**
- Define `ClientTransport` trait in `lp-client`
- Use Cargo features to gate transport implementations:
  - `feature = ["transport-websocket"]` → enables websocket transport
  - `feature = ["transport-serial"]` → enables serial transport
- All implementations in `lp-client` crate, gated by features

**Option C: Transport enum with feature gates**
- Define a `Transport` enum in `lp-client` with variants for each transport type
- Use Cargo features to gate enum variants:
  - `feature = ["transport-websocket"]` → `Transport::WebSocket` variant available
  - `feature = ["transport-serial"]` → `Transport::Serial` variant available
- Simpler than trait, but less flexible

**Suggested Course Forward:**
I recommend **Option A** because:
- Keeps `lp-client` core transport-agnostic (clean separation)
- Allows transport implementations in separate crates (matches user's note about ESP32 firmware)
- Follows pattern of `lp-shared/src/fs/` with feature gating
- Server-side websocket transport can live in `lp-server` or separate crate
- Client-side serial transport will be in ESP32 firmware (as user mentioned)

**DECIDED: Option A - Transport trait in lp-shared**

**Decision:**
- Define `ClientTransport` trait in `lp-shared` (separate from existing `Transport` trait in `lp-engine`)
- `lp-client` core is transport-agnostic, uses the trait
- Transport implementations in separate crates:
  - Server-side websocket transport (in `lp-server` or separate crate)
  - Client-side serial transport (in ESP32 firmware, not in `lp-client`)
- **Need to differentiate between server and client transport:**
  - Server transport: accepts multiple connections
  - Client transport: connects to one server
- Details of multi-connection handling will be addressed later
- Serialization format and async/sync details to be determined later

---

### Question 5: Server-side GetChanges Implementation

**Current State:**
- `ProjectRequest::GetChanges` requests changes since a frame with optional `detail_specifier`
- `ProjectResponse::GetChanges` includes `current_frame`, `engine_stats`, `node_detail`, and `node_handles`
- `ProjectRuntime` doesn't yet track frame IDs or node change frames
- `lp-server` needs to implement handling of `GetChanges` requests

**Question:**
How should `lp-server` implement `GetChanges`? We need to:
- Query `ProjectRuntime` for nodes that changed since `since_frame`
- Build `node_detail` map with node states
- Determine which nodes to include based on `detail_specifier` and change tracking
- Collect engine stats (frame timing, memory usage)

**DECIDED: Add methods to ProjectRuntime, server builds response**

**Decision:**
- Add methods to `ProjectRuntime`:
  - `get_current_frame(&self) -> FrameId` - returns current frame ID
  - `get_changed_nodes_since(&self, since_frame: FrameId) -> Vec<NodeHandle>` - returns handles of nodes where `min(last_config_frame, last_state_frame) > since_frame` OR `created_frame > since_frame`
    - Uses earlier of config or state update time (if config changed in frame 10 and state in frame 20, and asking since frame 15, include it)
  - `get_node_detail(&self, handle: NodeHandle) -> Option<NodeDetail>` - returns full node detail (path + state)
  - `get_engine_stats(&self) -> EngineStats` - returns frame timing and memory stats
- `lp-server` uses these methods to build `GetChanges` response:
  - Get current frame
  - Get changed nodes since `since_frame`
  - For each changed node, get detail
  - For nodes matching `detail_specifier`, always include detail (even if not changed)
  - Collect all current node handles for pruning (deleted nodes are those not in the list)
- First sync (`since_frame = 0`): include all nodes

---

### Question 6: Memory Tracking and Client State

**Current State:**
- `EngineStats` includes `memory_max_usage` and `memory_avg_usage` (u64)
- User mentioned wanting memory tracking data
- `lp-client` needs to track its own state (last synced frame, known nodes, etc.)

**Question:**
What memory tracking do we need, and how should `lp-client` manage its state?

**DECIDED: LpClient owns RemoteProject state directly**

**Decision:**
- **LpClient** - top-level client for server operations:
  - `projects: HashMap<ProjectHandle, RemoteProject>` - owns all loaded project state
  - `list_projects()` - list projects on server
  - `create_project(path)` - create new project
  - `load_project(path)` - load a project remotely, returns `ProjectHandle`
  - `unload_project(handle)` - unload project remotely, removes from map
  - `update_project(handle)` - sync project state from server (GetChanges), updates `RemoteProject`, returns `&RemoteProject`
  - `get_project(handle)` - get last known state without updating
- **RemoteProject** - project state (owned by LpClient):
  - `path: String` - project path
  - `config: ProjectConfig` - project configuration
  - `last_frame_id: FrameId` - last synced frame
  - `watched_nodes: HashSet<NodeHandle>` - nodes being watched (for detail requests)
  - `nodes: HashMap<NodeHandle, RemoteNode>` - node state
- **RemoteNode** - node state:
  - `path: String` - node path
  - `config: NodeConfig` - node configuration
  - `config_ver: FrameId` - last frame config was updated (for reloading config from disk)
  - `state_ver: FrameId` - last frame state was updated
  - `detail: Option<NodeDetail>` - optional node detail (when watched or recently changed)
- No separate `ProjectClient` - all operations through `LpClient` methods
- Client state can be in-memory initially (persistence can be added later if needed)

**Note:** This design avoids borrowing issues - `LpClient` owns everything and provides methods to access/update state.
