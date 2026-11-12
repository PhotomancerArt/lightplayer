# Design: Project Commands

## Overview

Add support for project management commands in the client-server system:
- Load projects from the server filesystem
- Send messages to loaded projects (GetChanges)
- Close/unload projects
- List available projects (on disk) and loaded projects (in memory)

This builds on the filesystem sync foundation and enables clients to manage and interact with projects running on the server.

## File Structure

```
lp-model/src/
├── message.rs                 # MODIFY: Add project variants to ClientRequest
├── server/
│   ├── api.rs                # MODIFY: Enable ProjectRequest variant in ServerResponse
│   └── fs_api.rs             # (no changes)
└── project/
    ├── api.rs                # MODIFY: Add SerializableNodeDetail, SerializableProjectResponse
    └── handle.rs             # (no changes)

lp-server/src/
├── lib.rs                    # (no changes)
├── server.rs                 # (no changes)
├── handlers.rs               # MODIFY: Implement project request handlers
├── project.rs                # (no changes)
├── project_manager.rs        # MODIFY: Switch to handle-based mapping, add handle generation
└── error.rs                  # (no changes)

lp-client/src/
├── lib.rs                    # (no changes)
├── client.rs                 # MODIFY: Add project management methods
├── error.rs                  # (no changes)
└── transport/
    └── memory.rs             # (no changes)

lp-client/tests/
└── project_sync.rs           # NEW: Integration tests for project commands
```

## Type Tree

### lp-model/src/message.rs
- `pub enum ClientRequest` - **MODIFY**: Add project management variants:
  ```rust
  pub enum ClientRequest {
      Filesystem(FsRequest),
      LoadProject { path: String },
      UnloadProject { handle: ProjectHandle },
      ProjectRequest { handle: ProjectHandle, request: ProjectRequest },
      ListAvailableProjects,
      ListLoadedProjects,
  }
  ```

### lp-model/src/server/api.rs
- `pub enum ServerResponse` - **MODIFY**: Enable `ProjectRequest` variant:
  ```rust
  pub enum ServerResponse {
      Filesystem(FsResponse),
      LoadProject { handle: ProjectHandle },
      UnloadProject,
      ProjectRequest { response: SerializableProjectResponse }, // ENABLED
      ListAvailableProjects { projects: Vec<AvailableProject> },
      ListLoadedProjects { projects: Vec<LoadedProject> },
  }
  ```

### lp-model/src/project/api.rs
- **NEW**: `pub enum SerializableNodeDetail` - Serializable wrapper for `NodeDetail`:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub enum SerializableNodeDetail {
      Texture {
          path: LpPath,
          config: TextureConfig,
          state: NodeState,
          status: NodeStatus,
      },
      Shader {
          path: LpPath,
          config: ShaderConfig,
          state: NodeState,
          status: NodeStatus,
      },
      Output {
          path: LpPath,
          config: OutputConfig,
          state: NodeState,
          status: NodeStatus,
      },
      Fixture {
          path: LpPath,
          config: FixtureConfig,
          state: NodeState,
          status: NodeStatus,
      },
  }
  ```

- **NEW**: `pub enum SerializableProjectResponse` - Serializable wrapper for `ProjectResponse`:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub enum SerializableProjectResponse {
      GetChanges {
          current_frame: FrameId,
          node_handles: Vec<NodeHandle>,
          node_changes: Vec<NodeChange>,
          node_details: BTreeMap<NodeHandle, SerializableNodeDetail>,
      },
  }
  ```

- **NEW**: Conversion functions:
  ```rust
  impl NodeDetail {
      pub fn to_serializable(&self) -> Result<SerializableNodeDetail, Error>;
  }
  
  impl ProjectResponse {
      pub fn to_serializable(&self) -> Result<SerializableProjectResponse, Error>;
  }
  ```

### lp-server/src/project_manager.rs
- **MODIFY**: `pub struct ProjectManager`:
  ```rust
  pub struct ProjectManager {
      /// Map of project handle -> Project instance
      projects: HashMap<ProjectHandle, Project>,
      /// Map of project name -> handle (for reverse lookup)
      name_to_handle: HashMap<String, ProjectHandle>,
      /// Next handle ID to assign (starts at 1)
      next_handle_id: u32,
      /// Base directory where projects are stored (relative path)
      projects_base_dir: String,
  }
  ```

- **MODIFY**: `pub fn load_project()` signature:
  ```rust
  pub fn load_project(
      &mut self,
      name: String,
      base_fs: &mut dyn LpFs,
      output_provider: Rc<RefCell<dyn OutputProvider>>,
  ) -> Result<ProjectHandle, ServerError>
  ```
  - Generate new `ProjectHandle` (increment `next_handle_id`)
  - Extract project name from path (last component)
  - Create project-scoped filesystem: `base_fs.chroot(&format!("{}/{}", projects_base_dir, name))`
  - Create `Project` instance with scoped filesystem
  - Auto-initialize project runtime:
    - `runtime.load_nodes()`
    - `runtime.init_nodes()`
    - `runtime.ensure_all_nodes_initialized()` (propagate errors)
  - Store handle -> project mapping
  - Store name -> handle mapping
  - Return handle

- **MODIFY**: `pub fn unload_project()` signature:
  ```rust
  pub fn unload_project(&mut self, handle: ProjectHandle) -> Result<(), ServerError>
  ```
  - Remove from `projects` HashMap
  - Remove from `name_to_handle` HashMap (find by iterating)

- **MODIFY**: `pub fn get_project()` and `get_project_mut()` signatures:
  ```rust
  pub fn get_project(&self, handle: ProjectHandle) -> Option<&Project>
  pub fn get_project_mut(&mut self, handle: ProjectHandle) -> Option<&mut Project>
  ```

- **MODIFY**: `pub fn list_loaded_projects()` return type:
  ```rust
  pub fn list_loaded_projects(&self) -> Vec<LoadedProject>
  ```
  - Returns `Vec<LoadedProject>` with handles and paths

- **NEW**: `pub fn get_handle_by_name()` helper:
  ```rust
  pub fn get_handle_by_name(&self, name: &str) -> Option<ProjectHandle>
  ```

### lp-server/src/handlers.rs
- **MODIFY**: `pub fn handle_client_message()` - Route project requests:
  ```rust
  pub fn handle_client_message(
      project_manager: &mut ProjectManager,
      base_fs: &mut dyn LpFs,
      output_provider: &Rc<RefCell<dyn OutputProvider>>,
      client_msg: ClientMessage,
  ) -> Result<ServerMessage, ServerError> {
      match client_msg.msg {
          ClientRequest::Filesystem(fs_request) => {
              // Existing filesystem handling
          }
          ClientRequest::LoadProject { path } => {
              handle_load_project(project_manager, base_fs, output_provider, path)
          }
          ClientRequest::UnloadProject { handle } => {
              handle_unload_project(project_manager, handle)
          }
          ClientRequest::ProjectRequest { handle, request } => {
              handle_project_request(project_manager, handle, request)
          }
          ClientRequest::ListAvailableProjects => {
              handle_list_available_projects(project_manager, base_fs)
          }
          ClientRequest::ListLoadedProjects => {
              handle_list_loaded_projects(project_manager)
          }
      }
  }
  ```

- **NEW**: `fn handle_load_project()`:
  - Extract project name from path (last component, strip `projects_base_dir` prefix if present)
  - Call `ProjectManager::load_project()`
  - Return `ServerResponse::LoadProject { handle }`

- **NEW**: `fn handle_unload_project()`:
  - Call `ProjectManager::unload_project(handle)`
  - Return `ServerResponse::UnloadProject`

- **NEW**: `fn handle_project_request()`:
  - Get project by handle
  - Match `ProjectRequest` variant:
    - `GetChanges { since_frame, detail_specifier }`:
      - Call `runtime.get_changes(since_frame, detail_specifier)`
      - Convert `ProjectResponse` to `SerializableProjectResponse`
      - Return `ServerResponse::ProjectRequest { response }`

- **NEW**: `fn handle_list_available_projects()`:
  - Call `ProjectManager::list_available_projects(base_fs)`
  - Convert names to `Vec<AvailableProject>` (with paths)
  - Return `ServerResponse::ListAvailableProjects { projects }`

- **NEW**: `fn handle_list_loaded_projects()`:
  - Call `ProjectManager::list_loaded_projects()`
  - Return `ServerResponse::ListLoadedProjects { projects }`

### lp-client/src/client.rs
- **NEW**: Project management methods:
  ```rust
  pub fn project_load(&mut self, path: String) -> (Message, u64)
  pub fn project_unload(&mut self, handle: ProjectHandle) -> (Message, u64)
  pub fn project_get_changes(
      &mut self,
      handle: ProjectHandle,
      since_frame: FrameId,
      detail_specifier: ApiNodeSpecifier,
  ) -> (Message, u64)
  pub fn project_list_available(&mut self) -> (Message, u64)
  pub fn project_list_loaded(&mut self) -> (Message, u64)
  ```

- **NEW**: Response extractor methods:
  ```rust
  pub fn extract_load_project_response(
      &mut self,
      request_id: u64,
      response: ServerResponse,
  ) -> Result<ProjectHandle, ClientError>
  
  pub fn extract_unload_project_response(
      &mut self,
      request_id: u64,
      response: ServerResponse,
  ) -> Result<(), ClientError>
  
  pub fn extract_get_changes_response(
      &mut self,
      request_id: u64,
      response: ServerResponse,
  ) -> Result<SerializableProjectResponse, ClientError>
  
  pub fn extract_list_available_projects_response(
      &mut self,
      request_id: u64,
      response: ServerResponse,
  ) -> Result<Vec<AvailableProject>, ClientError>
  
  pub fn extract_list_loaded_projects_response(
      &mut self,
      request_id: u64,
      response: ServerResponse,
  ) -> Result<Vec<LoadedProject>, ClientError>
  ```

## Design Decisions

### 1. Client Request Structure
**Decision**: Direct mapping - `ClientRequest` has same variants as `ServerRequest`
- Simpler structure, no extra nesting
- Consistent with filesystem requests
- Easier to route on server side

### 2. Project Handle Generation
**Decision**: Sequential IDs starting from 1, incrementing on each load (no reuse)
- Simple and deterministic (good for tests)
- No need to track freed handles
- Handles are opaque identifiers

### 3. Project Path vs Name
**Decision**: `LoadProject` path is relative to `projects_base_dir`, extract name from last component
- Simpler - just extract the last path component
- Paths are relative to `projects_base_dir` (consistent with `ProjectManager`)
- If path includes `projects_base_dir` prefix, strip it first

### 4. Project Initialization
**Decision**: Auto-initialize on load
- Projects ready to use immediately after loading
- Simpler API - clients don't need to remember to initialize
- If initialization fails, the load fails (clear error)

### 5. Project Filesystem Scoping
**Decision**: Use `chroot()` to create a scoped filesystem from `base_fs`
- `LpFs` already supports this via `chroot()` method
- Clean separation - project filesystem is scoped to project directory
- `ProjectRuntime` doesn't need to know about server filesystem structure

### 6. ProjectRequest Serialization
**Decision**: Implement GetChanges with serializable wrapper enum
- Create `SerializableNodeDetail` enum that matches on `NodeKind`
- Create `SerializableProjectResponse` that uses `SerializableNodeDetail`
- Conversion functions `NodeDetail -> SerializableNodeDetail` (match on `NodeKind`, downcast config)
- All concrete config types already implement `Serialize`/`Deserialize`

### 7. ProjectManager Handle Mapping
**Decision**: Primary mapping by handle, secondary mapping name -> handle
- Handles are the primary identifier for loaded projects
- Name -> handle mapping is just for convenience
- Cleaner API - methods take handles, not names

### 8. Integration Test Helpers
**Decision**: Put helpers in test file initially, extract later if needed
- Keep helpers focused and simple - each does one thing
- Can extract to shared module if useful for other tests

## Path Extraction Logic

For `LoadProject` path extraction:
1. If path starts with `projects_base_dir` prefix, strip it
2. Extract last component as project name
3. Examples:
   - `projects_base_dir = "projects"`, path = `"projects/my-project"` -> name = `"my-project"`
   - `projects_base_dir = "projects"`, path = `"my-project"` -> name = `"my-project"`
   - `projects_base_dir = "projects"`, path = `"projects/nested/my-project"` -> name = `"my-project"`

## Error Handling

- `LoadProject`: Returns `ServerError::ProjectNotFound` if project doesn't exist on filesystem
- `LoadProject`: Returns `ServerError::Core` if project initialization fails
- `UnloadProject`: Returns `ServerError::ProjectNotFound` if handle doesn't exist
- `ProjectRequest`: Returns `ServerError::ProjectNotFound` if handle doesn't exist
- `ProjectRequest::GetChanges`: Returns `ServerError::Core` if conversion to serializable fails

## Testing Strategy

### Integration Tests (`lp-client/tests/project_sync.rs`)

Helper functions:
- `create_test_project_on_client(fs: &mut dyn LpFs) -> String` - Creates project using `ProjectBuilder`, returns name
- `sync_project_to_server(...)` - Writes all project files to server via FS operations
- `load_project_on_server(...) -> ProjectHandle` - Sends LoadProject request, processes messages, returns handle
- `verify_project_loaded(server: &LpServer, handle: ProjectHandle) -> bool` - Checks if project is loaded
- `verify_project_running(server: &mut LpServer, handle: ProjectHandle) -> Result<(), Error>` - Checks if project can tick

Test cases:
1. `test_project_load_unload()` - Load project, verify loaded, unload, verify unloaded
2. `test_project_list_operations()` - List available, load, list loaded
3. `test_project_lifecycle()` - Create on client, sync to server, load, verify running, unload
4. `test_project_get_changes()` - Load project, send GetChanges request, verify response
5. `test_project_serialization()` - Verify SerializableNodeDetail and SerializableProjectResponse round-trip

## Success Criteria

- [ ] Client can send `LoadProject` request and receive handle
- [ ] Client can send `UnloadProject` request and project is unloaded
- [ ] Client can list available projects
- [ ] Client can list loaded projects with handles
- [ ] Client can send `GetChanges` request and receive serialized response
- [ ] Integration test creates project on client, syncs to server, loads it, verifies it's running
- [ ] All tests pass
- [ ] Code compiles without warnings

## Notes

- `SerializableNodeDetail` conversion requires downcasting `Box<dyn NodeConfig>` to concrete types using `as_any()` and `Any::downcast_ref()`
- Project auto-initialization ensures projects are ready to use immediately after loading
- Handle generation is sequential and deterministic for easier testing
- `chroot()` creates an immutable filesystem view, which is fine since `ProjectRuntime` only reads from filesystem (changes come from external sources)
