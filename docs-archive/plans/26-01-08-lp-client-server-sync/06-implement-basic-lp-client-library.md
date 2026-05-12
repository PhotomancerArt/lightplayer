# Phase 6: Implement Basic lp-client Library

## Goal

Create the basic `lp-client` library structure with `LpClient` and state structures.

## Tasks

1. Update `lp-client/Cargo.toml`:
   - Add dependencies: `lp-shared`, `serde`, `serde_json`, `hashbrown`
   - Set up crate structure

2. Create `lp-client/src/lib.rs`:
   - Export `client`, `project` modules

3. Create `lp-client/src/project.rs`:
   - Define `ProjectInfo` struct
   - Define `RemoteProject` struct with fields:
     - `path: String`
     - `config: ProjectConfig`
     - `last_frame_id: FrameId`
     - `watched_nodes: HashSet<NodeHandle>`
     - `nodes: HashMap<NodeHandle, RemoteNode>`
   - Define `RemoteNode` struct with fields:
     - `path: String`
     - `config: NodeConfig` (enum or trait object - to be determined)
     - `config_ver: FrameId`
     - `state_ver: FrameId`
     - `detail: Option<NodeDetail>`

4. Create `lp-client/src/client.rs`:
   - Define `LpClient<T: ClientTransport>` struct with:
     - `transport: T`
     - `projects: HashMap<ProjectHandle, RemoteProject>`
     - `next_request_id: u64`
   - Implement `new(transport: T) -> Self`
   - Implement `list_projects(&mut self) -> Result<Vec<ProjectInfo>, Error>` (stub for now)
   - Implement `create_project(&mut self, path: String) -> Result<(), Error>` (stub for now)
   - Implement `load_project(&mut self, path: String) -> Result<ProjectHandle, Error>` (stub for now)
   - Implement `unload_project(&mut self, handle: ProjectHandle) -> Result<(), Error>` (stub for now)
   - Implement `get_project(&self, handle: ProjectHandle) -> Option<&RemoteProject>`

5. Add error types:
   - Create `lp-client/src/error.rs` if needed
   - Or use `lp-shared` error types

## Success Criteria

- `lp-client` crate compiles
- `LpClient` struct exists with transport generic
- `RemoteProject` and `RemoteNode` structures are defined
- Basic methods exist (can be stubs)
- All code compiles without warnings
