# Implementation Plan: Engine Core and Model

## Philosophy

- **Minimal first**: Get a fully working system ASAP, then iterate
- **Use `todo!()`**: Mark unfinished sections with `todo!()` macro instead of comments
- **Incremental**: Small steps that keep things compiling and testable

## Goal

Get a working end-to-end test that:

- Loads a project from memory filesystem
- Initializes nodes (texture + shader + fixture + output)
- Renders a frame
- Syncs with a client
- Hot-reloads a shader file change

## Implementation Steps

### Phase 1: lp-model - Core Types (Minimal)

**Goal**: Basic types needed for everything else

1. **Path and FrameId**

   - `path.rs`: `LpPath(String)` newtype
   - `project/frame_id.rs`: `FrameId(i64)` newtype
   - `project/mod.rs`: Export both

2. **Node Types**

   - `nodes/kind.rs`: `NodeKind` enum (Texture, Shader, Output, Fixture)
   - `nodes/handle.rs`: `NodeHandle(i32)` newtype
   - `nodes/specifier.rs`: `NodeSpecifier(String)` newtype
   - `nodes/mod.rs`: Export all

3. **Project Config**

   - `project/config.rs`: `ProjectConfig { uid, name }` (minimal, no nodes field)
   - Update `project/mod.rs` to export

4. **Node Configs (Minimal)**

   - `nodes/texture/config.rs`: `TextureConfig` enum with one variant (Memory)
   - `nodes/shader/config.rs`: `ShaderConfig { glsl_path, texture_spec, render_order }`
   - `nodes/output/config.rs`: `OutputConfig` enum with one variant (GpioStrip)
   - `nodes/fixture/config.rs`: `FixtureConfig { output_spec, texture_spec, mapping, lamp_type, transform }`
   - Use `todo!()` for unimplemented variants/fields

5. **Node States (Minimal)**

   - `nodes/texture/state.rs`: `TextureState { texture_data: Vec<u8> }`
   - `nodes/shader/state.rs`: `ShaderState { glsl_code: String, error: Option<String> }`
   - `nodes/output/state.rs`: `OutputState { channel_data: Vec<u8> }`
   - `nodes/fixture/state.rs`: `FixtureState { lamp_colors: Vec<u8> }`
   - Use `todo!()` for complex fields

6. **Node Config Trait**
   - `nodes/mod.rs`: `NodeConfig` trait with `kind() -> NodeKind`
   - Implement for all config types

**Test**: Compiles, basic type tests

---

### Phase 2: lp-model - API Types (Minimal)

**Goal**: Types for client sync (can stub initially)

1. **Project API**
   - `project/api.rs`:
     - `NodeSpecifier` enum (None, All, ByHandles, ByPaths) - use `todo!()` for complex variants
     - `ProjectRequest::GetChanges { since_frame, detail_specifier }`
     - `ProjectResponse::GetChanges { current_frame, node_handles, node_changes, node_details }`
     - `NodeChange` enum (Created, ConfigUpdated, StateUpdated, StatusChanged, Removed)
     - `NodeDetail` struct (path, config, state, status)
   - Use `todo!()` for complex serialization/deserialization

**Test**: Compiles, can construct types

---

### Phase 3: lp-engine - Project Loading (Minimal)

**Goal**: Load project and discover nodes

1. **Error Type**

   - `error.rs`: `Error` enum (minimal variants: Io, Parse, NotFound, etc.)
   - Use `todo!()` for complex error handling

2. **Project Loader**

   - `project/loader.rs`:
     - `load_from_filesystem(fs: &LpFs) -> Result<ProjectConfig>`
     - `discover_nodes(fs: &LpFs) -> Result<Vec<LpPath>>`
     - `load_node(fs: &LpFs, path: &LpPath) -> Result<(LpPath, Box<dyn NodeConfig>)>`
   - Use `todo!()` for complex node type detection/parsing

3. **Project Runtime (Skeleton)**

   - `project/runtime.rs`:
     - `ProjectRuntime { frame_id, fs, nodes: HashMap<NodeHandle, NodeEntry>, next_handle }`
     - `NodeEntry { path, kind, config, config_ver, status, runtime: None, state_ver }`
     - `NodeStatus` enum (Created, InitError, Ok, Warn, Error)
     - `load_nodes()` - creates entries, doesn't initialize yet
   - Use `todo!()` for initialization logic

4. **Module Structure**
   - `lib.rs`: Export error, project modules
   - `project/mod.rs`: Export runtime, loader

**Test**: Can load a project from `LpFsMemory` and discover nodes

---

### Phase 4: lp-engine - Node Runtime Traits (Minimal)

**Goal**: Define runtime interfaces

1. **Node Runtime Trait**

   - `nodes/mod.rs`:
     - `NodeRuntime` trait with `init()` and `render()` (stub `destroy()` with `todo!()`)
     - `NodeConfig` trait re-export from `lp-model`

2. **Runtime Contexts**

   - `runtime/contexts.rs`:
     - `NodeInitContext` trait (stub methods with `todo!()`)
     - `RenderContext` trait (stub methods with `todo!()`)
   - Use `todo!()` for complex resolution logic

3. **Module Structure**
   - `nodes/mod.rs`: Export trait
   - `runtime/mod.rs`: Export contexts

**Test**: Compiles, can define trait objects

---

### Phase 5: lp-engine - Basic Node Runtimes (Minimal)

**Goal**: Stub implementations that compile

1. **Texture Runtime**

   - `nodes/texture/runtime.rs`: `TextureRuntime` struct
   - Implement `NodeRuntime` with `todo!()` in methods
   - `nodes/texture/mod.rs`: Export

2. **Shader Runtime**

   - `nodes/shader/runtime.rs`: `ShaderRuntime` struct
   - Implement `NodeRuntime` with `todo!()` in methods
   - `nodes/shader/mod.rs`: Export

3. **Output Runtime**

   - `nodes/output/runtime.rs`: `OutputRuntime` struct
   - Implement `NodeRuntime` with `todo!()` in methods
   - `nodes/output/mod.rs`: Export

4. **Fixture Runtime**
   - `nodes/fixture/runtime.rs`: `FixtureRuntime` struct
   - Implement `NodeRuntime` with `todo!()` in methods
   - `nodes/fixture/mod.rs`: Export

**Test**: Compiles, can create runtime instances

---

### Phase 6: lp-engine - Node Initialization (Minimal)

**Goal**: Initialize nodes in order

1. **Update ProjectRuntime**

   - `project/runtime.rs`:
     - `initialize_nodes()` - calls `init()` on each runtime
     - Order: textures → shaders → fixtures → outputs
     - On error, set status to `InitError` but keep entry
     - Use `todo!()` for complex resolution logic

2. **Update NodeEntry**
   - Store `runtime: Option<Box<dyn NodeRuntime>>` after init

**Test**: Can initialize nodes (even if they do nothing)

---

### Phase 7: lp-engine - Basic Rendering (Minimal)

**Goal**: Render a frame (stub actual work)

1. **Update ProjectRuntime**

   - `project/runtime.rs`:
     - `tick()` - increments frame_id
     - `render()` - iterates fixtures, calls `render()` on each
     - Use `todo!()` for texture lazy rendering, output flushing

2. **Update FixtureRuntime**
   - `nodes/fixture/runtime.rs`:
     - `render()` - calls `get_texture()` and `get_output()` (stub with `todo!()`)

**Test**: Can call `tick()` and `render()` without crashing

---

### Phase 8: lp-engine - Sync API (Minimal)

**Goal**: Implement `GetChanges` for client sync

1. **Update ProjectRuntime**
   - `project/runtime.rs`:
     - `get_changes(since_frame, detail_specifier) -> ProjectResponse`
     - Track changes in `NodeEntry` (config_ver, state_ver)
     - Use `todo!()` for complex change tracking

**Test**: Can call `get_changes()` and get response

---

### Phase 9: lp-engine-client - Client View (Minimal)

**Goal**: Client that can sync with engine

1. **Client Project View**

   - `project/view.rs`:
     - `ClientProjectView { frame_id, nodes, detail_tracking }`
     - `ClientNodeEntry { path, kind, config, config_ver, state, state_ver, status }`
     - `request_detail()`, `stop_detail()`, `detail_specifier()`
     - `sync()` - calls API and updates view
     - Use `todo!()` for complex sync logic

2. **Client API Trait**

   - `api/client.rs`: `ClientApi` trait with `get_changes()`
   - `api/mod.rs`: Export

3. **Module Structure**
   - `lib.rs`: Export project, api modules
   - `project/mod.rs`: Export view, sync

**Test**: Can create client view and sync with engine

---

### Phase 10: End-to-End Test

**Goal**: Full working test

1. **Test Helper**

   - `lp-engine/test_util/builder.rs`:
     - `TestProjectBuilder` - builds project in memory fs
     - Helper functions: `basic_texture()`, `basic_shader()`, `basic_output()`, `basic_fixture()`

2. **Integration Test**
   - Test in `lp-engine/tests/`:
     - Create project with texture + shader + fixture + output
     - Load and initialize
     - Render a frame
     - Create client, sync
     - Update shader file
     - Manually trigger reload (or use `todo!()` for filesystem watching)
     - Verify client sees update

**Test**: Full test passes

---

## Notes

- Use `todo!()` liberally for:

  - Complex error handling
  - Filesystem watching
  - Shader compilation
  - Texture lazy rendering
  - Output flushing
  - Complex node resolution
  - Serialization/deserialization

- Keep it compiling: Each phase should compile (even if tests fail)

- Test incrementally: Add tests as you go, don't wait until the end

- Iterate safely: Once the test passes, we can fill in `todo!()` sections incrementally
