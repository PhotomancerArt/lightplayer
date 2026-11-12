# Design Notes: LP-CLI Server and Client Modes

## Goal

Design the high-level architecture for running lightplayer locally on a desktop machine in server or client mode. This is a precursor to running the server on ESP32 and the client locally, allowing us to work out implementation issues without flashing a microcontroller.

## Current State

- **lp-cli**: Currently just a stub (`println!("Hello, world!")`)
- **lp-server**: Library exists with `LpServer` struct, `ProjectManager`, message handlers
- **lp-client**: Library exists with `LpClient` struct, supports filesystem operations and project management
- **Transport**: `ClientTransport` and `ServerTransport` traits exist in `lp-shared`, but only in-memory transport implemented
- **Message Protocol**: Exists in `lp-model` with `ClientMessage`, `ServerMessage`, `Message` envelope
- **Project Structure**: Projects are directories with `project.json` containing `{ uid, name }`
- **Server Structure**: Server containers should have `server.json` (currently just a placeholder)

## Requirements

### Server Mode
- `lp-cli serve <dir>` - Run server from a directory
- Default `<dir>` to "." but require `server.json` to exist (error if missing)
- `--init` flag creates directory and `server.json` file
- `--memory` flag starts server with temporary in-memory filesystem
- Uses websockets API (custom default port to avoid conflicts)

### Client Mode
- `lp-cli dev <host> <dir>` - Connect to server and sync local project
- `<host>` is a specifier (e.g., `ws://localhost:8080/`, `serial:auto`, `serial:/dev/ttyUSB1`)
- Default assumes `--push` (push local project to remote)
- Requires `project.json` in `<dir>` (defaults to working directory)
- Remote project name determined from `uid` in `project.json`
- Future: support `--pull` and bidirectional sync

### Project Creation
- Need to determine best approach for creating new projects
- Options: `lp-cli create <dir>`, `lp-cli dev --init`, or `lp-cli dev --create`

## Questions

### Question 1: CLI Library and Command Structure

**Current State:**
- `lp-cli` is currently just a stub
- Need to support multiple commands: `serve`, `dev`, `create`, `flash` (future)
- Commands have different argument structures and options

**Question:**
What CLI parsing library should we use, and how should we structure the command-based architecture?

**Options:**

**Option A: Use `clap` with derive macros**
```rust
#[derive(Parser)]
#[command(name = "lp-cli")]
enum Cli {
    Serve {
        dir: Option<PathBuf>,
        #[arg(long)]
        init: bool,
        #[arg(long)]
        memory: bool,
    },
    Dev {
        host: String,
        dir: Option<PathBuf>,
        #[arg(long)]
        push: bool,
        #[arg(long)]
        pull: bool,
    },
    Create {
        dir: PathBuf,
    },
}
```
- Most popular Rust CLI library
- Rich features (validation, help generation, etc.)
- Good for complex command structures
- Adds dependency

**Option B: Use `clap` with builder API**
- More explicit, less macro magic
- Still adds dependency

**Option C: Manual argument parsing**
- No dependencies
- More code to maintain
- Less feature-rich

**Suggested Course Forward:**
I recommend **Option A** (clap with derive macros) because:
- Standard Rust CLI library
- Rich features for validation and help
- Clean command structure matches requirements
- Dependency is acceptable for a CLI tool

**DECIDED: Option A - Use `clap` with derive macros**

---

### Question 2: Server Container Structure and server.json

**Current State:**
- Server containers should have `server.json` file (placeholder)
- Server manages projects in `projects/` subdirectory
- Need to determine what `server.json` should contain

**Question:**
What should the structure of `server.json` be, and what should it contain?

**Options:**

**Option A: Minimal placeholder**
```json
{}
```
- Just indicates directory is a server container
- No configuration needed initially

**Option B: Basic config**
```json
{
  "port": 8080,
  "projects_dir": "projects"
}
```
- Allows configuration of server settings
- More flexible

**Option C: Just metadata**
```json
{
  "version": "1.0"
}
```
- Version tracking
- Minimal but extensible

**Suggested Course Forward:**
I recommend **Option A** (minimal placeholder) initially because:
- Simplest to start with
- Can add fields later as needed
- Focus is on getting server/client working, not configuration

**DECIDED: Option A - Minimal placeholder with proper struct**
- Create `LpServerConfig` struct (empty fields for now)
- Add comments documenting what will go there (memory limits, security rules, etc.)
- Serializes to `{}` JSON for now
- For now, just indicates directory is a server container

---

### Question 3: Host Specifier Parsing and Transport Selection

**Current State:**
- Need to support multiple transport types: `ws://localhost:8080/`, `serial:auto`, `serial:/dev/ttyUSB1`
- Transport traits exist but only in-memory transport implemented
- Need to parse specifier and create appropriate transport

**Question:**
How should we parse host specifiers and select/create the appropriate transport implementation?

**Options:**

**Option A: Simple string parsing with enum**
```rust
enum HostSpecifier {
    WebSocket { url: String },
    Serial { port: Option<String> }, // None = auto
}

impl HostSpecifier {
    fn parse(s: &str) -> Result<Self, Error> {
        if s.starts_with("ws://") || s.starts_with("wss://") {
            Ok(HostSpecifier::WebSocket { url: s.to_string() })
        } else if s.starts_with("serial:") {
            let port = s.strip_prefix("serial:").map(|s| s.to_string());
            Ok(HostSpecifier::Serial { port })
        } else {
            Err(Error::InvalidHostSpecifier)
        }
    }
}
```
- Simple and explicit
- Easy to extend with new transport types
- Clear error handling

**Option B: Use URL parsing library**
- More robust URL parsing
- Handles edge cases better
- Adds dependency

**Option C: Regex-based parsing**
- More flexible
- Harder to maintain
- Overkill for simple specifiers

**Suggested Course Forward:**
I recommend **Option A** (simple string parsing with enum) because:
- Simple and explicit
- Easy to understand and maintain
- No additional dependencies
- Can switch to URL parsing later if needed

**DECIDED: Option A - Simple string parsing with enum**

---

### Question 4: WebSocket Transport Implementation

**Current State:**
- Transport traits exist (`ClientTransport`, `ServerTransport`)
- Only in-memory transport implemented
- Need websocket transport for server and client

**Question:**
What websocket library should we use, and how should we implement the websocket transport?

**Options:**

**Option A: `tokio-tungstenite`**
- Async websocket library
- Requires async runtime (tokio)
- Standard choice for Rust websockets
- Good for server (can handle multiple connections)

**Option B: `tungstenite` (sync)**
- Synchronous websocket library
- No async runtime needed
- Can work with polling interface
- Simpler for single-connection client

**Option C: `axum` with websocket support**
- Full web framework
- Built-in websocket support
- Overkill if we only need websockets

**Suggested Course Forward:**
I recommend **Option B** (`tungstenite` sync) initially because:
- Matches the polling-based transport interface
- No async runtime dependency
- Simpler for initial implementation
- Can buffer messages internally to match polling interface
- Can add async wrapper later if needed

However, for the server side, we may need async to handle multiple connections. We might need different approaches for client vs server.

**DECIDED:**
- Client: Use synchronous `tungstenite` (matches polling interface)
- Server: Can use async/std in `lp-cli` (it's fine to use std there)
- If websocket implementations go in `lp-shared`, they need to be feature-gated
- Transport design has been refactored to work directly with `ClientMessage`/`ServerMessage` types

---

### Question 5: Project Creation Command Structure

**Current State:**
- Need to support creating new projects
- Projects require `project.json` with `uid` and `name`
- User mentioned uncertainty about best approach

**Question:**
How should project creation work? Should it be:
- A separate command: `lp-cli create <dir>`
- A flag on dev: `lp-cli dev --init` or `lp-cli dev --create`
- Both?

**Options:**

**Option A: Separate `create` command**
```bash
lp-cli create <dir> --name "My Project"
```
- Clear separation of concerns
- Explicit project creation step
- Can create project without connecting to server

**Option B: Flag on `dev` command**
```bash
lp-cli dev <host> <dir> --create --name "My Project"
```
- Creates project and immediately connects
- Fewer commands to remember
- Less flexible

**Option C: Both**
- `create` for standalone project creation
- `dev --init` for create-and-connect workflow
- Most flexible but more to maintain

**Suggested Course Forward:**
I recommend **Option A** (separate `create` command) because:
- Clear separation: creation vs connection
- Can create projects independently
- Matches user's mention of `lp-cli create <dir>`
- Simpler mental model

**DECIDED: Option A - Separate `create` command**

---

### Question 6: Default Port for WebSocket Server

**Current State:**
- Need to choose a default port for websocket server
- Should avoid conflicts with common ports

**Question:**
What should the default port be for the websocket server?

**Options:**

**Option A: 8080**
- Common development port
- Might conflict with other services

**Option B: 8765** (or similar uncommon port)
- Less likely to conflict
- Less standard

**Option C: 8081** or **8082**
- Common alternative to 8080
- Still might conflict

**Suggested Course Forward:**
I recommend **Option B** (uncommon port like 8765) because:
- Less likely to conflict
- Can be overridden via config if needed
- Clear that it's a custom service

**DECIDED: Port 2812**
- Easy to remember (WS2812 LED)
- Not commonly used
- Can be overridden via config if needed

---

### Question 7: Server Directory Structure

**Current State:**
- Server runs from a directory
- Projects stored in `projects/` subdirectory
- `server.json` at root

**Question:**
What should the server directory structure look like?

**Options:**

**Option A: Flat structure**
```
server-dir/
├── server.json
└── projects/
    ├── project1/
    │   ├── project.json
    │   └── src/
    └── project2/
        ├── project.json
        └── src/
```

**Option B: With logs/config**
```
server-dir/
├── server.json
├── projects/
│   └── ...
└── logs/
    └── ...
```

**Suggested Course Forward:**
I recommend **Option A** (flat structure) initially because:
- Simplest to start with
- Can add subdirectories later as needed
- Focus on core functionality first

**DECIDED:**
- Start with flat structure (server.json, projects/)
- Directory paths (like logs/, projects/) should be configurable in server.json
- This makes it more flexible and configurable rather than hardcoding paths

---

### Question 8: Client Push Logic - Project Name Resolution

**Current State:**
- Client needs to push local project to server
- Project name on server should match `uid` from `project.json`
- Need to handle case where project doesn't exist on server

**Question:**
How should the client determine the remote project name, and what should happen if the project doesn't exist on the server?

**Options:**

**Option A: Use `uid` from project.json**
- Simple and explicit
- `uid` is meant to be unique identifier
- Server creates project if it doesn't exist

**Option B: Use `name` from project.json**
- More human-readable
- Might have conflicts
- Server creates project if it doesn't exist

**Option C: Allow override via flag**
```bash
lp-cli dev <host> <dir> --project-name <name>
```
- Most flexible
- Defaults to `uid` or `name`

**Suggested Course Forward:**
I recommend **Option A** (use `uid` from project.json) because:
- `uid` is designed to be unique identifier
- Matches user's suggestion
- Server can create project if it doesn't exist (via LoadProject or similar)
- Can add override flag later if needed

**DECIDED: Option A - Use `uid` from project.json**
- Server automatically creates project if it doesn't exist when pushing
- Can add override flag later if needed

---

### Question 9: File Structure for lp-cli

**Current State:**
- `lp-cli` is currently just `src/main.rs` with a stub
- Need to organize code for multiple commands

**Question:**
How should we structure the `lp-cli` codebase to support multiple commands?

**Options:**

**Option A: Commands as separate modules**
```
lp-cli/src/
├── main.rs
├── commands/
│   ├── mod.rs
│   ├── serve.rs
│   ├── dev.rs
│   └── create.rs
├── transport/
│   ├── mod.rs
│   ├── websocket.rs
│   └── serial.rs (future)
└── server_config.rs
```

**Option B: Everything in main.rs initially**
- Simpler to start
- Can refactor later
- Gets messy quickly

**Option C: Separate binaries for each command**
- More modular
- Harder to share code
- Overkill

**Suggested Course Forward:**
I recommend **Option A** (commands as separate modules) because:
- Clean organization from the start
- Easy to extend with new commands
- Good separation of concerns
- Matches common CLI app patterns

**DECIDED: Option A - Commands as separate modules**

---

### Question 10: Error Handling and User Experience

**Current State:**
- Need to handle various error cases (missing files, connection failures, etc.)
- Should provide clear error messages

**Question:**
How should we handle errors and provide good user experience?

**Options:**

**Option A: Use `anyhow` for error handling**
- Good error context
- Easy to use
- Adds dependency

**Option B: Use `thiserror` for custom errors**
- More control
- Type-safe
- More boilerplate

**Option C: Simple `Result` with string errors**
- No dependencies
- Less structured
- Harder to handle programmatically

**Suggested Course Forward:**
I recommend **Option A** (`anyhow`) because:
- Great for CLI applications
- Easy error context and messages
- Standard choice for Rust CLIs
- Dependency is acceptable

**DECIDED: Option A - Use `anyhow` for error handling**

---

## Notes

- Focus on getting basic server/client working first, can refine UX later
- WebSocket transport is priority, serial can come later
- Server should be able to run in foreground (not daemon) for development
- Client should support interactive mode (watch for changes and sync) vs one-shot sync
