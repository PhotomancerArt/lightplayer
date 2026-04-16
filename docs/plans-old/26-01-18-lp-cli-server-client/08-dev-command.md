# Phase 8: Implement Dev Command and Project Push Logic

## Goal

Implement the `dev` command to connect to server and push local project, with testable core logic.

## Tasks

1. Create `lp-app/apps/lp-cli/src/commands/dev/args.rs`:
   - Define `DevArgs` struct:
     ```rust
     pub struct DevArgs {
         pub host: String,
         pub dir: Option<PathBuf>,
         pub push: bool,
     }
     ```
   - Parse from `clap` arguments (push defaults to true)

2. Create `lp-app/apps/lp-cli/src/commands/dev/push.rs`:
   - `pub fn validate_local_project(dir: &Path) -> Result<(String, String)>`:
     - Check for `project.json`
     - Read and parse `project.json`
     - Extract `uid` and `name`
     - Return error with helpful message if missing
   - `pub fn push_project(client: &mut LpClient, local_fs: &dyn LpFs, project_uid: &str) -> Result<()>`:
     - Check if project exists on server (via `ListAvailableProjects`)
     - If not exists, create project directory on server
     - Recursively read all files from local project
     - Write all files to server via filesystem API
     - Load project on server (via `LoadProject`)
     - Accept `LpClient` and filesystem as parameters for testability

3. Create `lp-app/apps/lp-cli/src/commands/dev/handler.rs`:
   - `pub fn handle_dev(args: DevArgs) -> Result<()>`:
     - Determine project directory (default to ".")
     - Call `validate_local_project()`
     - Parse host specifier
     - Create appropriate transport (websocket for now)
     - Create `LpClient` with transport
     - Call `push_project()`
     - Handle errors with helpful messages

4. Update `lp-app/apps/lp-cli/src/commands/dev/mod.rs`:
   - Export `handle_dev` function
   - Re-export submodules

5. Create `lp-app/apps/lp-cli/src/commands/dev/sync.rs`:
   - Stub for future watch-and-sync functionality
   - Can be empty for now

6. Add tests:
   - Test project validation (missing project.json)
   - Test project push with memory filesystem
   - Test project push with in-memory transport
   - Test error cases (connection failures, etc.)

## Success Criteria

- `lp-cli dev <host> <dir>` connects to server
- Validates local project.json exists
- Pushes project files to server
- Loads project on server
- Shows helpful error if project.json missing
- Code is testable with memory filesystem and in-memory transport
- Tests pass
- Code compiles without warnings
