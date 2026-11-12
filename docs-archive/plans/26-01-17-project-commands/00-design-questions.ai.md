# Design Questions: Project Commands

## Scope

Add support for project management commands in the client-server system:
- Load projects from the server filesystem
- Send messages to loaded projects (GetChanges for now, more later)
- Close/unload projects
- List available projects (on disk) and loaded projects (in memory)

For integration tests, we'll create a project on the client FS, sync it to the server, load it, send messages to it, and verify it works. We want good helper functions to make the tests clean.

## Current State

### What Exists
- **ProjectManager** (`lp-server/src/project_manager.rs`):
  - `load_project(name: String, fs: Box<dyn LpFs>, output_provider: Rc<RefCell<dyn OutputProvider>>)` - loads by name, uses `name: String` -> `Project` mapping
  - `unload_project(name: &str)` - unloads by name
  - `list_loaded_projects()` - returns `Vec<String>` (names only)
  - `list_available_projects(fs: &dyn LpFs)` - returns `Vec<String>` (names only)
  - `get_project(name: &str)` and `get_project_mut(name: &str)` - access by name

- **Project** (`lp-server/src/project.rs`):
  - Wraps `ProjectRuntime` with name and path
  - `runtime_mut()` and `runtime()` for accessing the runtime
  - `Project::new()` creates a `ProjectRuntime` but doesn't initialize nodes

- **ProjectHandle** (`lp-model/src/project/handle.rs`):
  - Opaque handle type `ProjectHandle(u32)` for identifying loaded projects
  - Already defined but not used by `ProjectManager`

- **Message Protocol** (`lp-model/src/server/api.rs`):
  - `ServerRequest` enum already has project variants:
    - `LoadProject { path: String }`
    - `UnloadProject { handle: ProjectHandle }`
    - `ProjectRequest { handle: ProjectHandle, request: ProjectRequest }`
    - `ListAvailableProjects`
    - `ListLoadedProjects`
  - `ServerResponse` enum has corresponding variants:
    - `LoadProject { handle: ProjectHandle }`
    - `UnloadProject`
    - `ProjectRequest { response: ProjectResponse }` (disabled - see TODO)
    - `ListAvailableProjects { projects: Vec<AvailableProject> }`
    - `ListLoadedProjects { projects: Vec<LoadedProject> }`

- **ClientRequest** (`lp-model/src/message.rs`):
  - Currently only has `Filesystem(FsRequest)` variant
  - Missing project management variants

- **ProjectRuntime** (`lp-engine/src/project/runtime.rs`):
  - `new()` - loads project config
  - `load_nodes()` - discovers and loads node configs from filesystem
  - `init_nodes()` - initializes nodes in dependency order
  - `ensure_all_nodes_initialized()` - verifies all nodes initialized successfully
  - `tick(delta_ms)` - advances frame and renders
  - `get_changes(since_frame, specifier)` - returns `ProjectResponse::GetChanges` (but serialization disabled)

### What's Missing
- `ClientRequest` doesn't have project management variants
- `ProjectManager` uses name-based mapping instead of handle-based
- `ProjectManager` doesn't generate `ProjectHandle` values
- Server handlers for project requests (currently just placeholder)
- Client-side methods for project operations
- Integration tests with helper functions

## Questions

### Question 1: Client Request Structure

**Current State:**
- `ClientRequest` only has `Filesystem(FsRequest)` variant
- `ServerRequest` already has project variants defined
- Need to add project management to `ClientRequest`

**Question:**
Should project management requests in `ClientRequest` map directly to `ServerRequest` variants, or should they be nested differently?

**Options:**
- **Option A**: Direct mapping - `ClientRequest` has same variants as `ServerRequest`:
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
- **Option B**: Nested under a `ProjectManagement` wrapper:
  ```rust
  pub enum ClientRequest {
      Filesystem(FsRequest),
      ProjectManagement(ProjectManagementRequest),
  }
  pub enum ProjectManagementRequest {
      LoadProject { path: String },
      UnloadProject { handle: ProjectHandle },
      // ...
  }
  ```

**Suggested Course Forward:**
I recommend **Option A** (direct mapping) because:
- Simpler structure - no extra nesting
- Consistent with how filesystem requests work
- Easier to route on server side
- Matches the existing `ServerRequest` structure

**DECIDED: Option A - Direct mapping**

---

### Question 2: Project Handle Generation

**Current State:**
- `ProjectHandle` is defined as `ProjectHandle(u32)`
- `ProjectManager` currently uses `name: String` -> `Project` mapping
- Need to generate unique handles when projects are loaded

**Question:**
How should `ProjectHandle` values be generated? Should they be:
- Sequential IDs (1, 2, 3, ...)
- Random/hash-based
- Based on project name (hash of name)
- Reuse handles when projects are unloaded?

**Options:**
- **Option A**: Sequential IDs starting from 1, incrementing on each load
- **Option B**: Sequential IDs with reuse (track freed handles, reuse lowest available)
- **Option C**: Hash-based (e.g., hash of project name)
- **Option D**: Random UUID-style (but u32, so truncated)

**Suggested Course Forward:**
I recommend **Option A** (sequential, no reuse) because:
- Simple and deterministic (good for tests)
- No need to track freed handles
- Handles are opaque identifiers - clients don't need to know the pattern
- If we need reuse later, we can add it without breaking the API

**DECIDED: Option A - Sequential IDs, no reuse**

---

### Question 3: Project Path vs Name

**Current State:**
- `ServerRequest::LoadProject` takes a `path: String`
- `ProjectManager::load_project()` takes a `name: String`
- Server has `projects_base_dir` (e.g., `"projects/"`)

**Question:**
How should we reconcile the path parameter in `LoadProject` with the name-based `ProjectManager`?

**Options:**
- **Option A**: `LoadProject` path is relative to `projects_base_dir`, extract name from path:
  - Path: `"my-project"` or `"projects/my-project"` -> name: `"my-project"`
  - Path: `"projects/nested/my-project"` -> name: `"my-project"` (last component)
- **Option B**: `LoadProject` path is absolute from server root, extract relative to `projects_base_dir`:
  - Path: `"/projects/my-project"` -> relative: `"my-project"`
- **Option C**: `LoadProject` takes both `path` and `name` separately

**Suggested Course Forward:**
I recommend **Option A** (relative to `projects_base_dir`, extract name from last component) because:
- Simpler - just extract the last path component as the name
- Paths are relative to `projects_base_dir` (consistent with how `ProjectManager` works)
- If path includes `projects_base_dir` prefix, strip it first, then extract name
- Example: if `projects_base_dir` is `"projects"` and path is `"projects/my-project"`, extract `"my-project"`

**DECIDED: Option A - Relative path, extract name from last component**

---

### Question 4: Project Initialization

**Current State:**
- `Project::new()` creates a `ProjectRuntime` but doesn't initialize nodes
- `ProjectRuntime` requires calling `load_nodes()`, `init_nodes()`, `ensure_all_nodes_initialized()` separately
- Tests show this pattern: `runtime.load_nodes().unwrap(); runtime.init_nodes().unwrap(); runtime.ensure_all_nodes_initialized().unwrap();`

**Question:**
Should projects be automatically initialized when loaded, or should initialization be a separate step?

**Options:**
- **Option A**: Auto-initialize on load - call all three methods in `ProjectManager::load_project()`
- **Option B**: Separate `initialize()` method that must be called explicitly
- **Option C**: Lazy initialization - initialize on first use (e.g., first `tick()` or `get_changes()`)

**Suggested Course Forward:**
I recommend **Option A** (auto-initialize on load) because:
- Projects should be ready to use immediately after loading
- Simpler API - clients don't need to remember to initialize
- If initialization fails, the load fails (clear error)
- Matches the pattern used in tests

**DECIDED: Option A - Auto-initialize on load**

---

### Question 5: Project Filesystem Scoping

**Current State:**
- `LpServer` has a `base_fs: Box<dyn LpFs>` (server root, projects in `projects/` subdirectory)
- `ProjectManager::load_project()` takes `fs: Box<dyn LpFs>` 
- `ProjectRuntime` expects paths relative to project root (e.g., `/project.json`, `/src/shader.shader/main.glsl`)
- `LpFs` trait has `chroot(subdir: &str) -> Result<Box<dyn LpFs>, FsError>` method

**Question:**
How should we provide a project-scoped filesystem to `ProjectRuntime`?

**Options:**
- **Option A**: Use `chroot()` to create a scoped filesystem from `base_fs`:
  - `let project_fs = base_fs.chroot(&format!("{}/{}", projects_base_dir, name))?;`
  - Pass `project_fs` to `Project::new()`
- **Option B**: Pass the base filesystem and project path, let `ProjectRuntime` handle path prefixing
- **Option C**: Create a wrapper filesystem that prefixes paths

**Suggested Course Forward:**
I recommend **Option A** (use `chroot()`) because:
- `LpFs` already supports this via `chroot()` method
- Clean separation - project filesystem is scoped to project directory
- `ProjectRuntime` doesn't need to know about server filesystem structure
- Matches the design pattern used elsewhere (e.g., node directories)

**DECIDED: Option A - Use chroot() for project-scoped filesystem**

---

### Question 6: ProjectRequest Serialization

**Current State:**
- `ProjectResponse` contains `NodeDetail` which has `Box<dyn NodeConfig>` (trait object)
- Trait objects can't be serialized directly with serde
- `ServerResponse::ProjectRequest` variant is commented out with TODO
- `ProjectRequest::GetChanges` exists and works, but response can't be serialized

**Question:**
Should we implement `ProjectRequest::GetChanges` now, or defer it?

**Options:**
- **Option A**: Defer `GetChanges` - focus on load/unload/list operations first
- **Option B**: Implement `GetChanges` with a workaround (e.g., serializable wrapper, custom serializer)
- **Option C**: Implement `GetChanges` but return an error indicating serialization not yet supported

**Suggested Course Forward:**
I recommend **Option A** (defer `GetChanges`) because:
- Load/unload/list are foundational operations needed first
- Serialization issue is complex and deserves its own design work
- Can add `GetChanges` later without breaking existing code
- Tests can verify projects are loaded and running without `GetChanges` (e.g., check project state directly)

**DECIDED: Option B - Implement GetChanges with a workaround for serialization**

**Note**: User indicated GetChanges is needed to actually use/test projects, so we need to solve the serialization issue now.

**Approach**: Create a serializable wrapper enum for `NodeDetail` that matches on `NodeKind` and serializes concrete config types. All concrete config types (TextureConfig, ShaderConfig, OutputConfig, FixtureConfig) already implement `Serialize`/`Deserialize`.

Implementation plan:
1. Create `SerializableNodeDetail` enum in `lp-model/src/project/api.rs`:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub enum SerializableNodeDetail {
       Texture { path: LpPath, config: TextureConfig, state: NodeState, status: NodeStatus },
       Shader { path: LpPath, config: ShaderConfig, state: NodeState, status: NodeStatus },
       Output { path: LpPath, config: OutputConfig, state: NodeState, status: NodeStatus },
       Fixture { path: LpPath, config: FixtureConfig, state: NodeState, status: NodeStatus },
   }
   ```
2. Add conversion functions `NodeDetail -> SerializableNodeDetail` (match on `NodeKind`, downcast config)
3. Create `SerializableProjectResponse` that uses `SerializableNodeDetail` instead of `NodeDetail`
4. Use `SerializableProjectResponse` in `ServerResponse::ProjectRequest` variant
5. Keep `ProjectResponse` as-is for internal use (non-serializable)

---

### Question 7: ProjectManager Handle Mapping

**Current State:**
- `ProjectManager` uses `HashMap<String, Project>` (name -> project)
- Need to support `ProjectHandle` -> `Project` mapping
- Also need reverse lookup (name -> handle) for some operations

**Question:**
How should `ProjectManager` store projects - by handle, by name, or both?

**Options:**
- **Option A**: Primary mapping by handle, secondary mapping name -> handle:
  ```rust
  projects: HashMap<ProjectHandle, Project>,
  name_to_handle: HashMap<String, ProjectHandle>,
  ```
- **Option B**: Primary mapping by name, generate handle on access:
  ```rust
  projects: HashMap<String, Project>,
  name_to_handle: HashMap<String, ProjectHandle>, // generated on load
  ```
- **Option C**: Single mapping with both as keys (two HashMaps, same data)

**Suggested Course Forward:**
I recommend **Option A** (primary by handle) because:
- Handles are the primary identifier for loaded projects
- Name -> handle mapping is just for convenience (e.g., `list_loaded_projects()`)
- Cleaner API - methods take handles, not names
- If a project is renamed on disk, handle stays the same

**DECIDED: Option A - Primary mapping by handle, secondary name->handle mapping**

---

### Question 8: Integration Test Helpers

**Current State:**
- Tests in `lp-client/tests/fs_sync.rs` have helper functions like `setup_server_and_client()` and `process_messages()`
- `ProjectBuilder` exists in `lp-model/src/project/builder.rs` for creating test projects
- Need helpers for project operations in tests

**Question:**
What helper functions should we create for project integration tests?

**Suggested Helpers:**
- `create_test_project_on_client(fs: &mut dyn LpFs) -> String` - creates a project using `ProjectBuilder`, returns project name
- `sync_project_to_server(client: &mut LpClient, client_transport: &mut MemoryTransport, server_transport: &mut MemoryTransport, project_name: &str)` - writes all project files to server via FS operations
- `load_project_on_server(client: &mut LpClient, ...) -> ProjectHandle` - sends LoadProject request, processes messages, returns handle
- `verify_project_loaded(server: &LpServer, handle: ProjectHandle) -> bool` - checks if project is loaded
- `verify_project_running(server: &mut LpServer, handle: ProjectHandle) -> Result<(), Error>` - checks if project runtime is initialized and can tick

**Question:**
Should these helpers be in the test file, or in a shared test utilities module?

**Suggested Course Forward:**
- Put helpers in the test file initially (like `fs_sync.rs` does)
- If they're useful for other tests later, we can extract to a shared module
- Keep helpers focused and simple - each does one thing

**DECIDED: Put helpers in test file initially, extract to shared module later if needed**

---

## Summary

Key decisions needed:
1. Client request structure (direct mapping vs nested)
2. Handle generation strategy (sequential vs reuse vs hash)
3. Path vs name handling in LoadProject
4. Auto-initialization vs explicit initialization
5. Filesystem scoping approach (chroot vs wrapper)
6. Whether to defer GetChanges
7. ProjectManager storage structure (handle-first vs name-first)
8. Test helper organization
