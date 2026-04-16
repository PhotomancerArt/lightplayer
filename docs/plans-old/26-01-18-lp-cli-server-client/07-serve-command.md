# Phase 7: Implement Serve Command

## Goal

Implement the `serve` command to start a server with websocket API, accepting filesystem as parameter for testability.

## Tasks

1. Create `lp-app/apps/lp-cli/src/commands/serve/args.rs`:
   - Define `ServeArgs` struct:
     ```rust
     pub struct ServeArgs {
         pub dir: Option<PathBuf>,
         pub init: bool,
         pub memory: bool,
     }
     ```
   - Parse from `clap` arguments

2. Create `lp-app/apps/lp-cli/src/commands/serve/init.rs`:
   - `pub fn initialize_server(dir: &Path, init: bool) -> Result<ServerConfig>`:
     - Check if `server.json` exists
     - If `--init` flag and missing, create it
     - If missing and not `--init`, return error with helpful message
     - Load and return `ServerConfig`
   - `pub fn create_filesystem(dir: Option<&Path>, memory: bool) -> Result<Box<dyn LpFs>>`:
     - If `--memory`, return `LpFsMemory::new()`
     - Otherwise, return `LpFsStd::new(dir.unwrap_or("."))`
     - Accept filesystem as parameter for testability

3. Create `lp-app/apps/lp-cli/src/commands/serve/server_loop.rs`:
   - `pub fn run_server_loop(server: LpServer, transport: WebSocketServerTransport) -> Result<()>`:
     - Main server loop:
       - Accept new connections (non-blocking)
       - Poll all connections for incoming messages
       - Collect messages into `Vec<Message>`
       - Call `server.tick(messages)`
       - Route responses back to appropriate connections
     - Handle errors gracefully
     - Accept `LpServer` and transport as parameters for testability

4. Create `lp-app/apps/lp-cli/src/commands/serve/handler.rs`:
   - `pub fn handle_serve(args: ServeArgs) -> Result<()>`:
     - Determine server directory (default to ".")
     - Call `initialize_server()`
     - Call `create_filesystem()`
     - Create `LpServer` with filesystem
     - Create `WebSocketServerTransport` on port 2812
     - Call `run_server_loop()`

5. Update `lp-app/apps/lp-cli/src/commands/serve/mod.rs`:
   - Export `handle_serve` function
   - Re-export submodules

6. Add tests:
   - Test server initialization with `--init`
   - Test server initialization without `--init` (error case)
   - Test memory filesystem mode
   - Test server loop with in-memory transport (can use `LocalTransport`)

## Success Criteria

- `lp-cli serve <dir>` starts server
- `lp-cli serve --init` creates server.json if missing
- `lp-cli serve --memory` uses memory filesystem
- Server accepts websocket connections
- Server processes messages and sends responses
- Code is testable with memory filesystem
- Tests pass
- Code compiles without warnings
