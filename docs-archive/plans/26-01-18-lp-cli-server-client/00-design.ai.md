# Design: LP-CLI Server and Client Modes

## Overview

Design the high-level architecture for running lightplayer locally on a desktop machine in server or client mode. This enables development and testing of the client-server implementation before deploying to ESP32. The `lp-cli` application supports multiple commands:

- **Server mode**: `lp-cli serve <dir>` - Run server from a directory with websocket API
- **Client mode**: `lp-cli dev <host> <dir>` - Connect to server and sync local project
- **Create command**: `lp-cli create <dir>` - Create a new project

This builds on the existing `lp-server` and `lp-client` libraries and adds CLI interfaces, websocket transport, and project management workflows.

## File Structure

```
lp-app/apps/lp-cli/
├── Cargo.toml                    # MODIFY: Add dependencies (clap, anyhow, tokio, tungstenite, etc.)
└── src/
    ├── main.rs                   # MODIFY: CLI entry point with command parsing
    ├── commands/
    │   ├── mod.rs               # NEW: Command module exports
    │   ├── serve/
    │   │   ├── mod.rs           # NEW: Serve command module
    │   │   ├── args.rs          # NEW: Serve command arguments
    │   │   ├── handler.rs       # NEW: Serve command handler
    │   │   ├── server_loop.rs   # NEW: Server main loop logic
    │   │   └── init.rs          # NEW: Server initialization logic
    │   ├── dev/
    │   │   ├── mod.rs           # NEW: Dev command module
    │   │   ├── args.rs          # NEW: Dev command arguments
    │   │   ├── handler.rs       # NEW: Dev command handler
    │   │   ├── push.rs          # NEW: Project push logic
    │   │   └── sync.rs          # NEW: Sync/watch logic (future)
    │   └── create/
    │       ├── mod.rs           # NEW: Create command module
    │       ├── args.rs          # NEW: Create command arguments
    │       ├── handler.rs       # NEW: Create command handler
    │       └── project.rs       # NEW: Project creation logic
    ├── transport/
    │   ├── mod.rs               # NEW: Transport module exports
    │   ├── websocket/
    │   │   ├── mod.rs           # NEW: WebSocket module
    │   │   ├── client.rs        # NEW: Client websocket transport
    │   │   └── server.rs        # NEW: Server websocket transport
    │   └── specifier.rs         # NEW: Host specifier parsing
    ├── config/
    │   ├── mod.rs               # NEW: Config module
    │   └── server.rs            # NEW: ServerConfig struct
    ├── error.rs                 # NEW: CLI-specific error types
    └── messages.rs              # NEW: User-friendly message formatting helpers

lp-app/crates/lp-shared/src/transport/
├── mod.rs                       # (no changes)
├── client.rs                    # (no changes)
├── server.rs                    # (no changes)
├── local.rs                     # (no changes)
└── websocket.rs                 # NEW: WebSocket transport implementations (feature-gated)

lp-app/crates/lp-model/src/server/
├── api.rs                       # (no changes)
├── fs_api.rs                    # (no changes)
└── config.rs                    # NEW: ServerConfig struct for server.json
```

## Type Tree

### lp-app/apps/lp-cli/src/main.rs

- `fn main() -> Result<()>` - **MODIFY**: CLI entry point
  - Parse command-line arguments using `clap`
  - Route to appropriate command handler
  - Handle errors with `anyhow`

### lp-app/apps/lp-cli/src/commands/mod.rs

- Re-exports for `serve`, `dev`, `create` modules

### lp-app/apps/lp-cli/src/commands/serve/mod.rs

- Re-exports for serve submodules
- `pub fn serve(args: ServeArgs) -> Result<()>` - **NEW**: Main serve command entry point

### lp-app/apps/lp-cli/src/commands/serve/args.rs

- `pub struct ServeArgs` - **NEW**: Parsed serve command arguments
  - `dir: Option<PathBuf>`
  - `init: bool`
  - `memory: bool`

### lp-app/apps/lp-cli/src/commands/serve/handler.rs

- `pub fn handle_serve(args: ServeArgs) -> Result<()>` - **NEW**: Serve command handler
  - Orchestrates serve command execution

### lp-app/apps/lp-cli/src/commands/serve/init.rs

- `pub fn initialize_server(dir: &Path, init: bool) -> Result<ServerConfig>` - **NEW**: Server initialization
  - Validate/create server directory
  - Check/create `server.json`
  - Load/return `ServerConfig`

### lp-app/apps/lp-cli/src/commands/serve/server_loop.rs

- `pub fn run_server_loop(server: LpServer, transport: WebSocketServerTransport) -> Result<()>` - **NEW**: Server main loop
  - Accept connections
  - Poll transports
  - Call `LpServer::tick()`
  - Route responses

### lp-app/apps/lp-cli/src/commands/dev/mod.rs

- Re-exports for dev submodules
- `pub fn dev(args: DevArgs) -> Result<()>` - **NEW**: Main dev command entry point

### lp-app/apps/lp-cli/src/commands/dev/args.rs

- `pub struct DevArgs` - **NEW**: Parsed dev command arguments
  - `host: String`
  - `dir: Option<PathBuf>`
  - `push: bool`

### lp-app/apps/lp-cli/src/commands/dev/handler.rs

- `pub fn handle_dev(args: DevArgs) -> Result<()>` - **NEW**: Dev command handler
  - Orchestrates dev command execution
  - Validates project.json exists, shows helpful error if missing

### lp-app/apps/lp-cli/src/commands/dev/push.rs

- `pub fn push_project(client: &mut LpClient, local_dir: &Path, project_uid: &str) -> Result<()>` - **NEW**: Push project logic
  - Read local project files
  - Create project on server if needed
  - Write files to server
  - Load project on server

### lp-app/apps/lp-cli/src/commands/dev/sync.rs

- `pub fn watch_and_sync(...)` - **NEW**: Watch for changes and sync (future)
  - File watching logic
  - Incremental sync

### lp-app/apps/lp-cli/src/commands/create/mod.rs

- Re-exports for create submodules
- `pub fn create(args: CreateArgs) -> Result<()>` - **NEW**: Main create command entry point

### lp-app/apps/lp-cli/src/commands/create/args.rs

- `pub struct CreateArgs` - **NEW**: Parsed create command arguments
  - `dir: PathBuf`
  - `name: Option<String>` - Optional, defaults to directory name
  - `uid: Option<String>` - Optional, auto-generated if not provided

### lp-app/apps/lp-cli/src/commands/create/handler.rs

- `pub fn handle_create(args: CreateArgs) -> Result<()>` - **NEW**: Create command handler
  - Orchestrates create command execution

### lp-app/apps/lp-cli/src/commands/create/project.rs

- `pub fn create_project_structure(dir: &Path, name: Option<&str>, uid: Option<&str>) -> Result<()>` - **NEW**: Project creation logic
  - Derive `name` from directory if not provided
  - Generate `uid` with format `YYYY-MM-DDTHH.MM.SS-<name>` if not provided
  - Create directory structure
  - Generate `project.json`
  - Create `src/` directory
- `pub fn generate_uid(name: &str) -> String` - **NEW**: Generate UID from name
  - Format: `2025-01-15T12.15.02-my-project` (ISO-like with dots, name appended)
  - Uses current date/time
- `pub fn print_success_message(dir: &Path, name: &str)` - **NEW**: Print user-friendly success message
  - Shows next steps: `cd <name> && lp-cli dev ws://localhost:2812/`

### lp-app/apps/lp-cli/src/transport/specifier.rs

- `pub enum HostSpecifier` - **NEW**: Parsed host specifier
  ```rust
  pub enum HostSpecifier {
      WebSocket { url: String },
      Serial { port: Option<String> }, // None = auto
  }
  ```
- `impl HostSpecifier` - **NEW**: Parse from string
  - `pub fn parse(s: &str) -> Result<Self, Error>`

### lp-app/apps/lp-cli/src/transport/websocket/mod.rs

- Re-exports for websocket submodules

### lp-app/apps/lp-cli/src/transport/websocket/client.rs

- `pub struct WebSocketClientTransport` - **NEW**: Client websocket transport
  - Implements `ClientTransport`
  - Uses `tungstenite` (sync) with internal buffering
  - Handles serialization/deserialization

### lp-app/apps/lp-cli/src/transport/websocket/server.rs

- `pub struct WebSocketServerTransport` - **NEW**: Server websocket transport
  - Implements `ServerTransport`
  - Uses async websocket (tokio-tungstenite) for multiple connections
  - Wraps async in polling interface
- `pub struct Connection` - **NEW**: Individual websocket connection state

### lp-app/crates/lp-shared/src/transport/websocket.rs (feature-gated)

- `pub struct WebSocketClientTransport` - **NEW**: Client websocket transport
  - Feature: `std` or `websocket`
  - Uses `tungstenite` (sync)
- `pub struct WebSocketServerTransport` - **NEW**: Server websocket transport
  - Feature: `std` and `async` or `websocket-server`
  - Uses `tokio-tungstenite`

### lp-app/crates/lp-model/src/server/config.rs

- `pub struct ServerConfig` - **NEW**: Server configuration
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ServerConfig {
      // Future: memory_limits, security_rules, etc.
      // For now, empty struct serializes to {}
  }
  ```
  - Comments document future fields

### lp-app/apps/lp-cli/src/error.rs

- CLI-specific error types
- Error formatting with context and suggestions

### lp-app/apps/lp-cli/src/messages.rs

- `pub fn print_success(message: &str, next_steps: &[&str])` - **NEW**: Print success message with next steps
- `pub fn print_error(message: &str, suggestions: &[&str])` - **NEW**: Print error message with suggestions
- `pub fn format_command(cmd: &str) -> String` - **NEW**: Format command for copy-paste (with proper quoting)
- Helper functions for consistent message formatting

## Process Flow

### Server Startup Flow

```
lp-cli serve <dir>
    |
    +-- Parse arguments
    |   +-- dir (default: ".")
    |   +-- --init (create server.json if missing)
    |   +-- --memory (use in-memory filesystem)
    |
    +-- Validate/create server directory
    |   +-- Check for server.json
    |   +-- If --init: create server.json
    |   +-- If missing and not --init: error
    |
    +-- Initialize filesystem
    |   +-- If --memory: LpFsMemory
    |   +-- Else: LpFsStd with dir as root
    |
    +-- Create LpServer
    |   +-- OutputProvider (MemoryOutputProvider)
    |   +-- Base filesystem
    |   +-- Projects base dir: "projects/"
    |
    +-- Create WebSocketServerTransport
    |   +-- Bind to port 2812 (or from config)
    |   +-- Accept connections
    |
    +-- Server loop
        +-- Accept new connections
        +-- For each connection:
            +-- Receive messages via transport
            +-- Convert to Vec<Message>
            +-- Call LpServer::tick()
            +-- Send responses via transport
```

### Client Dev Flow

```
lp-cli dev <host> <dir>
    |
    +-- Parse arguments
    |   +-- host (e.g., "ws://localhost:2812/")
    |   +-- dir (default: ".")
    |   +-- --push (default: true)
    |
    +-- Parse host specifier
    |   +-- HostSpecifier::parse(host)
    |   +-- Create appropriate transport
    |
    +-- Validate local project
    |   +-- Check for project.json in dir
    |   +-- Read project.json, extract uid
    |
    +-- Create WebSocketClientTransport
    |   +-- Connect to server
    |
    +-- Create LpClient
    |
    +-- Push project to server
    |   +-- Read all files from local project
    |   +-- Create/ensure project exists on server (using uid)
    |   +-- Write files to server via filesystem API
    |   +-- Load project on server
    |
    +-- (Future: Watch for changes and re-sync)
```

### Project Creation Flow

```
lp-cli create <dir> [--name "My Project"] [--uid "custom-uid"]
    |
    +-- Parse arguments
    |   +-- dir (required)
    |   +-- --name (optional, defaults to directory name)
    |   +-- --uid (optional, auto-generate if missing)
    |
    +-- Derive defaults
    |   +-- name: from --name or directory name
    |   +-- uid: from --uid or generate with format YYYY.MM.DD-HH.MM.SS-<name>
    |
    +-- Create directory structure
    |   +-- Create dir/ (if doesn't exist)
    |   +-- Create dir/src/
    |
    +-- Generate project.json
    |   +-- uid: derived or provided
    |   +-- name: derived or provided
    |
    +-- Write project.json
    |
    +-- Print success message with next steps
```

## Design Decisions

### 1. CLI Library: clap with derive macros

**Decision**: Use `clap` with derive macros for command parsing

- Standard Rust CLI library
- Rich features for validation and help generation
- Clean command structure matches requirements
- Dependency is acceptable for a CLI tool

### 2. Server Configuration: Empty struct with documentation

**Decision**: `ServerConfig` struct with no fields, documented for future use

- Simplest to start with
- Can add fields later (memory limits, security rules, etc.)
- Comments document future fields
- Serializes to `{}` JSON

### 3. Host Specifier Parsing: Simple string parsing

**Decision**: Simple string parsing with enum, no URL library

- Simple and explicit
- Easy to extend with new transport types
- Clear error handling
- No additional dependencies
- Can switch to URL parsing later if needed

### 4. WebSocket Transport: Sync for client, async for server

**Decision**:

- Client: Synchronous `tungstenite` (matches polling interface)
- Server: Async `tokio-tungstenite` (handles multiple connections)
- Server-side async is fine in `lp-cli` (can use std)
- If implementations go in `lp-shared`, they must be feature-gated

### 5. Project Creation: Separate command with minimal input

**Decision**: `lp-cli create <dir>` as separate command with good defaults

- Clear separation: creation vs connection
- Can create projects independently
- Simpler mental model
- `--name` optional, defaults to directory name
- `--uid` optional, auto-generated with format `YYYY-MM-DDTHH.MM.SS-<name>`
- Minimal input required, sensible defaults

### 6. Default Port: 2812

**Decision**: Use port 2812 as default

- Easy to remember (WS2812 LED)
- Not commonly used
- Can be overridden via config if needed

### 7. Server Directory Structure: Configurable paths

**Decision**: Directory paths configurable in `server.json`

- Start with flat structure (server.json, projects/)
- Directory paths (like logs/, projects/) should be configurable
- More flexible than hardcoding paths

### 8. Project Name Resolution: Use uid from project.json

**Decision**: Use `uid` from `project.json` as remote project name

- `uid` is designed to be unique identifier
- Server automatically creates project if it doesn't exist when pushing
- Can add override flag later if needed

### 9. File Structure: Directory modules with many small files

**Decision**: Organize large commands as directory modules with many small files

- Each major command (`serve`, `dev`, `create`) is a directory module
- Functionality split into focused files (args, handler, specific logic)
- Clean organization from the start
- Easy to extend with new commands or functionality
- Good separation of concerns
- Matches preference for many-small-files approach

### 10. Error Handling: anyhow

**Decision**: Use `anyhow` for error handling

- Great for CLI applications
- Easy error context and messages
- Standard choice for Rust CLIs
- Dependency is acceptable

### 11. Testability: Generic interfaces and dependency injection

**Decision**: Write all code with testability in mind

- Use `LpFs` trait everywhere, allowing `LpFsMemory` for tests
- Accept filesystem and transport as parameters in handlers
- Separate core logic from CLI argument parsing
- Enable end-to-end tests with memory filesystem and in-memory transport
- No hardcoded paths or filesystem operations
- Transport abstraction allows swapping implementations for testing

## Implementation Notes

### WebSocket Transport Implementation

**Client Transport:**

- Uses synchronous `tungstenite`
- Buffers incoming messages internally to match polling interface
- Handles serialization/deserialization of `ClientMessage`/`ServerMessage`

**Server Transport:**

- Uses async `tokio-tungstenite` for handling multiple connections
- Wraps async operations in polling interface
- Maintains connection state per client
- Routes messages to/from appropriate connections

### Server Loop Architecture

The server runs a main loop that:

1. Accepts new websocket connections
2. For each connection, maintains a transport instance
3. Polls each transport for incoming messages
4. Collects all messages and passes to `LpServer::tick()`
5. Routes responses back to appropriate transports

**Testability**: The serve command handler should accept filesystem as a parameter (or create it based on flags), allowing tests to inject `LpFsMemory`. The server loop should accept `LpServer` and transport as parameters, allowing tests to use in-memory transport.

### Project Push Logic

When pushing a project:

1. Read `project.json` to get `uid`
2. Check if project exists on server (via `ListAvailableProjects`)
3. If not exists, create project directory structure on server
4. Recursively read all files from local project
5. Write all files to server via filesystem API
6. Load project on server (via `LoadProject`)

**Testability**: The push logic should accept `LpClient` and filesystem as parameters, allowing tests to use memory filesystem and in-memory transport. The dev command handler should create these based on arguments, but the core logic should be separate and testable.

### User-Friendly Messaging Implementation

All user-facing messages should:

- Use consistent formatting (✓ for success, ✗ for errors)
- Include copy-pasteable commands
- Show exact next steps

Helper functions in `messages.rs` format messages consistently:

- Success messages show what was accomplished and next steps
- Error messages show what went wrong and how to fix it
- Commands are formatted for easy copy-paste (with proper quoting)

### UID Generation Format

UIDs are generated with format: `YYYY-MM-DDTHH.MM.SS-<name>`

- Example: `2025-01-15T12.15.02-my-project`
- Uses ISO 8601-like format but with dots instead of colons (filesystem-friendly)
- Appends project name for readability
- Not guaranteed unique, but good enough for most cases
- Makes server directories nicely sorted by creation time

### Host Specifier Format

Supported formats:

- `ws://localhost:2812/` - WebSocket connection
- `wss://example.com/` - Secure WebSocket
- `serial:auto` - Serial connection (auto-detect port)
- `serial:/dev/ttyUSB1` - Serial connection (specific port)

Future formats:

- `serial:COM3` - Windows serial port
- `tcp://host:port` - Raw TCP (if needed)

## Error Handling

- **Missing server.json**: Error unless `--init` flag provided
  - Show exact command: `lp-cli serve <dir> --init`
- **Missing project.json**: Error in `dev` command
  - Show exact command: `lp-cli create <dir> --name "Project Name"` or `cd <dir> && lp-cli create . --name "Project Name"`
- **Invalid host specifier**: Clear error message with supported formats
  - Show examples: `ws://localhost:2812/`, `serial:auto`
- **Connection failures**: Retry logic (future) or clear error message
  - Show troubleshooting steps
- **Transport errors**: Wrapped in `anyhow::Error` with context
- **Server errors**: Propagated from `LpServer` with context

### User-Friendly Messaging

All success and error messages should:

- Tell the user exactly what to do next
- Provide copy-pasteable commands when applicable
- Be actionable and specific

**Examples:**

**Success after `create`:**

```
✓ Project created successfully: my-project (uid: 2025.01.15-12.15.02-my-project)

Next steps:
  cd my-project && lp-cli dev ws://localhost:2812/
```

**Error when `dev` without project.json:**

```
✗ No project.json found in current directory

To create a new project, run:
  lp-cli create .

Or to use an existing project:
  cd /path/to/project && lp-cli dev ws://localhost:2812/
```

**Error when `serve` without server.json:**

```
✗ No server.json found in /path/to/dir

To initialize a new server, run:
  lp-cli serve /path/to/dir --init
```

## Testing Strategy

### Testability Design

All server and client code should be written with testability in mind:

- **Generic filesystem**: Use `LpFs` trait, allowing `LpFsMemory` for tests
- **Dependency injection**: Accept filesystem and transport as parameters
- **No hardcoded paths**: All paths should be configurable
- **Transport abstraction**: Use transport traits, allowing in-memory transport for tests

This enables:

- Unit tests with memory filesystem
- Integration tests with in-memory transport
- End-to-end tests without real filesystem or network

### Unit Tests

- Host specifier parsing
- Server config serialization/deserialization
- Project creation logic
- UID generation

### Integration Tests

- Server startup with memory filesystem
- Client connection with in-memory transport
- Project push with memory filesystem
- End-to-end: create project, serve (memory), dev (in-memory transport), verify sync

### Manual Testing

- Test server with `--memory` flag
- Test server with filesystem
- Test client with various host specifiers
- Test project creation workflow

## Success Criteria

- [ ] `lp-cli serve <dir>` starts server and accepts websocket connections
- [ ] `lp-cli serve --init` creates server.json if missing
- [ ] `lp-cli serve --memory` runs with in-memory filesystem
- [ ] `lp-cli dev <host> <dir>` connects and pushes local project
- [ ] `lp-cli create <dir>` creates new project structure
- [ ] Server handles multiple client connections
- [ ] Client can push project and server loads it correctly
- [ ] All tests pass
- [ ] Code compiles without warnings

## Notes

- WebSocket transport implementations should be feature-gated if added to `lp-shared`
- Server can run in foreground (not daemon) for development
- Future: Client should support interactive mode (watch for changes and sync)
- Future: Support `--pull` flag for bidirectional sync
- Future: Serial transport implementation for ESP32
- Default port 2812 chosen for easy memory (WS2812 LED)
