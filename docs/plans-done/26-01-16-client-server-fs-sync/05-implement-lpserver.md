# Phase 5: Implement LpServer with tick() API

## Goal

Create `LpServer` struct with tick-based API that processes messages and handles filesystem and project operations.

## Tasks

1. Create `lp-server/src/server.rs`:

   - Define `LpServer` struct:
     ```rust
     pub struct LpServer {
         output_provider: Rc<RefCell<dyn OutputProvider>>,
         project_manager: ProjectManager,
         base_fs: Box<dyn LpFs>,
     }
     ```
   - Implement `new()`:
     ```rust
     pub fn new(
         output_provider: Rc<RefCell<dyn OutputProvider>>,
         base_fs: Box<dyn LpFs>,
         projects_base_dir: String,
     ) -> Self
     ```
   - Implement `tick()`:
     ```rust
     pub fn tick(
         &mut self,
         delta_ms: u32,
         incoming: Vec<Message>,
     ) -> Result<Vec<Message>, ServerError>
     ```

2. Create `lp-server/src/handlers.rs`:

   - Implement `handle_fs_request()`:
     - Match `FsRequest` variants
     - Call appropriate `LpFs` methods on `base_fs`
     - Build `FsResponse` with results or errors
   - Implement `handle_project_request()`:
     - Match `ServerRequest` project variants
     - Call `ProjectManager` methods
     - Build `ServerResponse` with results
   - Implement `handle_message()`:
     - Extract `ServerMessage` from `Message::Server`
     - Match `ServerRequest` variants
     - Call appropriate handler
     - Wrap response in `ServerMessage` with matching ID

3. Update `tick()` implementation:

   - Process each incoming message
   - Collect responses
   - Return `Vec<Message>` with `Server` variants

4. Update `lp-server/src/project.rs`:

   - Change `Project::new()` signature to take `Rc<RefCell<dyn OutputProvider>>`
   - Remove creation of `MemoryOutputProvider` inside `Project::new()`

5. Update `lp-server/src/lib.rs`:

   - Export `server` module
   - Export `LpServer` struct

6. Update `ProjectManager`:

   - Ensure it can work with `LpServer`'s `base_fs`
   - `projects_base_dir` should be relative to server root (e.g., "projects/")

7. Add error handling:
   - Convert `FsError` to `ServerError`
   - Handle transport errors appropriately

## Success Criteria

- `LpServer` struct exists with `new()` and `tick()` methods
- `tick()` processes messages and returns responses
- Filesystem operations work on `base_fs`
- Project operations work through `ProjectManager`
- `Project::new()` takes `OutputProvider` parameter
- All code compiles without warnings
