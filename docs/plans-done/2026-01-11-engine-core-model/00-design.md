# Engine Core and Model Architecture Design

## Overview

This design refactors `lp-engine` to have a cleaner architecture, moving shared config and state model from `lp-shared` to `lp-model`, and creating a new `ProjectRuntime` structure that supports loading projects, initializing nodes, and rendering scenes.

## File Structure

```
lp-model/
└── src/
    ├── lib.rs
    ├── path.rs                    # NEW: LpPath newtype wrapper
    ├── project/
    │   ├── mod.rs
    │   ├── config.rs              # ProjectConfig (moved from lp-shared)
    │   └── frame_id.rs             # FrameId (moved from lp-shared)
    └── nodes/
        ├── mod.rs
        ├── kind.rs                 # NEW: NodeKind enum
        ├── handle.rs               # NodeHandle (moved from lp-shared)
        ├── specifier.rs            # NEW: NodeSpecifier newtype wrapper
        ├── texture/
        │   ├── mod.rs
        │   ├── config.rs           # TextureConfig (moved from lp-shared)
        │   └── state.rs            # TextureState (moved from lp-shared)
        ├── shader/
        │   ├── mod.rs
        │   ├── config.rs           # ShaderConfig (moved from lp-shared)
        │   └── state.rs            # ShaderState (moved from lp-shared)
        ├── output/
        │   ├── mod.rs
        │   ├── config.rs           # OutputConfig (moved from lp-shared)
        │   └── state.rs            # OutputState (moved from lp-shared)
        └── fixture/
            ├── mod.rs
            ├── config.rs           # FixtureConfig (moved from lp-shared)
            └── state.rs            # FixtureState (moved from lp-shared)

lp-engine/
└── src/
    ├── lib.rs
    ├── error.rs                    # Error type
    ├── project/
    │   ├── mod.rs
    │   ├── runtime.rs              # ProjectRuntime, NodeEntry, NodeStatus
    │   └── loader.rs               # load_from_filesystem, discover_nodes, load_node
    ├── nodes/
    │   ├── mod.rs                  # NodeRuntime trait
    │   ├── texture/
    │   │   ├── mod.rs
    │   │   └── runtime.rs          # TextureRuntime
    │   ├── shader/
    │   │   ├── mod.rs
    │   │   └── runtime.rs          # ShaderRuntime
    │   ├── output/
    │   │   ├── mod.rs
    │   │   └── runtime.rs          # OutputRuntime
    │   └── fixture/
    │       ├── mod.rs
    │       └── runtime.rs          # FixtureRuntime
    ├── runtime/
    │   ├── mod.rs
    │   └── contexts.rs             # NodeInitContext, FixtureRenderContext
    └── test_util/
        ├── mod.rs
        └── builder.rs              # Test project builders

lp-engine-client/
└── src/
    ├── lib.rs
    ├── project/
    │   ├── mod.rs
    │   ├── view.rs                 # ClientProjectView, ClientNodeEntry
    │   └── sync.rs                 # Sync protocol implementation
    └── api/
        ├── mod.rs
        └── client.rs               # Client API trait
```

## Type and Function Summary

```
lp-model::

path::
  LpPath(String)                    # Newtype wrapper, absolute paths for now
                                    # Designed to support relative paths later

project::
  ProjectConfig { uid, name }       # Moved from lp-shared
  FrameId(i64)                      # Newtype wrapper, moved from lp-shared

  api::
    NodeSpecifier {
      None,                          # No nodes
      All,                           # All nodes
      ByHandles(Vec<NodeHandle>),    # Specific handles
      ByPaths(Vec<LpPath>)          # Specific paths (resolved server-side)
    }

    ProjectRequest {
      GetChanges {
        since_frame: FrameId,
        detail_specifier: NodeSpecifier  # Which nodes need full state
      }
    }

    ProjectResponse {
      GetChanges {
        current_frame: FrameId,
        node_handles: Vec<NodeHandle>,     # All current nodes
        node_changes: Vec<NodeChange>,     # Changed nodes since since_frame
        node_details: HashMap<NodeHandle, NodeDetail>  # Full detail for requested nodes
      }
    }

    NodeChange {
      Created { handle, path, kind },
      ConfigUpdated { handle, config_ver },
      StateUpdated { handle, state_ver },
      StatusChanged { handle, status },
      Removed { handle }
    }

    NodeDetail {
      path: LpPath,
      config: Box<dyn NodeConfig>,
      state: NodeState,              # External state
      status: NodeStatus
    }

nodes::
  NodeKind { Texture, Shader, Output, Fixture }
                                    # Matches directory suffixes

  NodeHandle(i32)                   # Runtime identifier, sequential generation
                                    # Moved from lp-shared

  NodeSpecifier(String)             # Newtype wrapper for node references
                                    # Currently just a path, may support other
                                    # specifier types in the future

  texture::
    TextureConfig { size, format }  # Enum variants (Memory, etc.)
    TextureState { texture_data }   # Runtime values

  shader::
    ShaderConfig {
      glsl_path,                    # File path (config)
      texture_spec,                 # NodeSpecifier
      render_order                  # Default 0, lower = render first
    }
    ShaderState {
      glsl_code,                    # Actual code (state)
      error                         # Compilation/runtime errors
    }

  output::
    OutputConfig { impl }           # Enum variants (GpioStrip, etc.)
    OutputState { channel_data }    # Runtime buffer values

  fixture::
    FixtureConfig {
      output_spec,                  # NodeSpecifier
      texture_spec,                 # NodeSpecifier
      mapping,                      # Mapping configuration
      lamp_type,                    # Color order, etc.
      transform                     # Matrix4x4
    }
    FixtureState {
      lamp_colors                   # Runtime color values
    }

lp-engine::

error::
  Error { ... }                     # Error enum

project::
  ProjectRuntime {
    frame_id: FrameId,
    fs: LpFs,                       # Owned? (TBD - may need mut, possibly unsafe)
    nodes: HashMap<NodeHandle, NodeEntry>,
    next_handle: i32
  }

  NodeEntry {
    path: LpPath,
    kind: NodeKind,
    config: Box<dyn NodeConfig>,    # Trait object for config
    config_ver: FrameId,             # Frame when config was last updated
    status: NodeStatus,
    runtime: Option<Box<dyn NodeRuntime>>,
    state_ver: FrameId              # Last frame updates occurred
  }

  NodeStatus {
    Created,                        # Created but not yet initialized
    InitError(String),              # Error initializing the node
    Ok,                             # Node is running normally
    Warn(String),                   # Node is running, but something is wrong
    Error(String)                   # Node cannot run
  }

  loader::
    load_from_filesystem(fs: &LpFs) -> Result<ProjectConfig>
    discover_nodes(fs: &LpFs) -> Result<Vec<LpPath>>
    load_node(fs: &LpFs, path: &LpPath) -> Result<(LpPath, NodeConfig)>

nodes::
  NodeRuntime trait {
    fn init(ctx: &NodeInitContext) -> Result<()>
    fn render(ctx: &RenderContext) -> Result<()>
    fn destroy() -> Result<()>
  }

  NodeConfig trait {
    fn kind() -> NodeKind
  }

  texture::TextureRuntime
  shader::ShaderRuntime
  output::OutputRuntime
  fixture::FixtureRuntime

runtime::
  contexts::
    NodeInitContext trait {
      resolve_output(spec: NodeSpecifier) -> Result<OutputHandle>
      resolve_texture(spec: NodeSpecifier) -> Result<TextureHandle>
      get_node_fs() -> &LpFs
    }

    FixtureRenderContext trait {
      get_texture(handle: TextureHandle) -> Result<TextureData>
                                      # Lazy rendering: triggers texture render
                                      # if state_ver < frame_id
      get_output(handle: OutputHandle, universe, start_ch, ch_count)
        -> Result<&mut [u8]>
                                      # Marks output as updated this frame
    }

test_util::
  builder::
    TestProjectBuilder              # Builder for test projects
    basic_texture() -> TextureConfig
    basic_shader() -> ShaderConfig
    basic_output() -> OutputConfig
    basic_fixture() -> FixtureConfig

lp-engine-client::

project::
  ClientProjectView {
    frame_id: FrameId,
    nodes: HashMap<NodeHandle, ClientNodeEntry>,
    detail_tracking: HashSet<NodeHandle>  # Which nodes we're tracking detail for
  }

  ClientNodeEntry {
    path: LpPath,
    kind: NodeKind,
    config: Box<dyn NodeConfig>,
    config_ver: FrameId,
    state: Option<NodeState>,       # Only present if in detail_tracking
    state_ver: FrameId,
    status: NodeStatus
  }

  view::
    request_detail(handles: Vec<NodeHandle>)  # Start tracking detail for nodes
    stop_detail(handles: Vec<NodeHandle>)     # Stop tracking detail for nodes
    detail_specifier() -> NodeSpecifier       # Generate specifier for sync
    sync(api: &dyn ClientApi) -> Result<()>   # Sync with server

api::
  ClientApi trait {
    get_changes(request: ProjectRequest) -> Result<ProjectResponse>
  }
```

## Key Design Decisions

### Path Representation

- `LpPath` is a newtype wrapper around `String`
- Currently supports absolute paths only (from project root)
- Designed to support relative paths later when nodes become nestable

### Node Specifiers

- `NodeSpecifier` is a newtype wrapper around `String` for semantic clarity
- Currently just a path string, but may support other specifier types in the future
- Used for resolving node references at init/config time

### Handle Generation

- `NodeHandle` is a runtime identifier, generated sequentially
- Handles can change on reload (not stable)
- Paths are for loading/resolving; handles are for runtime references

### Node Lifecycle

- Failed nodes remain in the map with error status (`InitError`, `Error`)
- Even missing `node.json` creates an entry with error status
- This makes errors visible in UI and allows references to resolve to failed nodes
- "Show must go on" philosophy - all processing at node level, don't stop on errors

### Config vs State

- Config: "how the state is computed" (file paths, expressions, references)
- State: actual runtime values (loaded code, computed values)
- Runtime should almost fully replicate config, but they serve different purposes
- Initial state derived from config during init
- In the future, configs may reference other nodes (expressions), but state always has current used value

### Texture Rendering

- Lazy rendering: textures only render when requested via `get_texture()`
- When requested, check `state_ver < frame_id` and render if needed
- Shaders ordered by `render_order` (lower = render first, default 0)
- If shader returns `Skip`, try next shader in order

### Filesystem Abstraction

- `ProjectRuntime` owns `LpFs` (exact ownership TBD - may need `mut`, possibly unsafe)
- Allows filesystem operations during `tick()` for hot-reloading

### Testing

- Test-data building pattern with helper functions
- Functions generate pre-built builders for testing with basic nodes
- Can be customized per test
- Use `LpFsMemory` for filesystem-based tests

## Initialization Flow

1. `load_from_filesystem()` reads `/project.json` to get `ProjectConfig`
2. `discover_nodes()` scans `/src/` for node directories (`.shader`, `.texture`, `.output`, `.fixture`)
3. `load_node()` loads each node's `node.json` and related files (e.g., `main.glsl` for shaders)
4. `ProjectRuntime::load_nodes()` creates `NodeEntry` for each discovered node:
   - Generate handle
   - Load and parse config (on error, set status to `InitError`)
   - Create `NodeEntry` with `status: Created`
5. Initialize nodes in order: textures → shaders → fixtures → outputs
   - Call `init()` on each node runtime
   - On error, set status to `InitError` but keep entry
   - On success, set status to `Ok`

## Render Flow

1. `ProjectRuntime::tick()` increments `frame_id`
2. Iterate fixture nodes and call `render()` on each
3. Fixture's `render()` calls `get_texture()`:
   - If texture's `state_ver < frame_id`, render it:
     - Find shaders targeting this texture
     - Sort by `render_order` (lower first)
     - Call `render()` on each shader until one succeeds (not `Skip`)
     - Update texture's `state_ver` to current frame
   - Return texture data
4. Fixture samples texture and calls `get_output()`:
   - Get output buffer slice
   - Mark output's `state_ver` to current frame
   - Write channel data
5. At end of frame, flush outputs with `state_ver == frame_id`

## Client Architecture

### Overview

The engine uses a dedicated client architecture (`lp-engine-client`) rather than a follower/leader model. This keeps the client lightweight - it doesn't need the full rendering pipeline, shader compilation, or texture buffers. It only needs:

- Node configs (for editing)
- External state (for viewing)
- Node status (for debugging)

### Client View Model

The client maintains a lightweight view of the project:

- **`ClientProjectView`**: Tracks the current state of the project from the client's perspective

  - Maintains a map of all nodes (configs + status)
  - Tracks which nodes are being viewed (`detail_tracking`)
  - Only stores full state (external state) for nodes being tracked

- **`ClientNodeEntry`**: Represents a node in the client view
  - Always has config and status (needed for UI)
  - Only has state if the node is in `detail_tracking`
  - Tracks version numbers for incremental updates

### Selective Sync

The client uses selective sync to minimize bandwidth:

1. **Detail Tracking**: Client explicitly requests detail for specific nodes via `request_detail()`

   - Typically nodes visible in the UI
   - Can add/remove nodes dynamically as user navigates

2. **Incremental Updates**: Client requests changes since last sync via `GetChanges`

   - Only gets full state for nodes in `detail_tracking`
   - Gets change notifications for all nodes (config updates, status changes)
   - Uses `since_frame` to get incremental updates

3. **State Management**: When stopping detail tracking, client clears state to save memory
   - Config and status remain (needed for UI)
   - Only external state is cleared

### Sync Protocol

The sync protocol is defined in `lp-model/src/project/api.rs`:

- **`ProjectRequest::GetChanges`**: Client requests changes since a frame

  - `since_frame`: Last frame client synced
  - `detail_specifier`: Which nodes need full state (from `detail_tracking`)

- **`ProjectResponse::GetChanges`**: Server responds with changes

  - `current_frame`: Current frame ID
  - `node_handles`: All current node handles (for pruning removed nodes)
  - `node_changes`: List of changes since `since_frame`
  - `node_details`: Full detail (config + state) for requested nodes

- **`NodeChange`**: Represents a change to a node
  - `Created`: New node added
  - `ConfigUpdated`: Config changed (client may need to fetch new config)
  - `StateUpdated`: State changed (client gets state if tracking detail)
  - `StatusChanged`: Status changed
  - `Removed`: Node removed

### Editing Workflow

1. Client edits config locally in `ClientNodeEntry`
2. Client writes changes back via filesystem API (`FsWrite` in `lp-api`)
3. Server detects filesystem change and hot-reloads the node
4. Client gets update via `GetChanges` on next sync
5. Client updates local view

### Debugging

Client can request additional data on demand:

- Texture data: Request via `GetTextureData` (one-off, not synced)
- Output state: Request via `GetOutputState` (one-off, not synced)
- Node detail: Can temporarily add to `detail_tracking` to get state

### Why Not Follower Mode?

A follower engine instance would be:

- **Heavier**: Needs full runtime (shaders, textures, outputs)
- **More complex**: Leader/follower sync is harder than incremental state sync
- **Less efficient**: Syncs everything, not just what's needed
- **Less flexible**: Clients can't easily connect/disconnect

The dedicated client approach is:

- **Lightweight**: Only syncs what's needed
- **Flexible**: Clients can connect/disconnect without affecting engine
- **Efficient**: Selective sync minimizes bandwidth
- **Simple**: Incremental state sync is straightforward

## Open Questions

- Exact `LpFs` ownership model in `ProjectRuntime` (may need `mut`, possibly unsafe)
