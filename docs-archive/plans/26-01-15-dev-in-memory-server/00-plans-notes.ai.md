# Plan Notes: Dev Command In-Memory Server

## Context

Currently, the `lp-cli dev` command requires a `host` parameter to connect to a remote server. For development and testing, users must run `lp-cli serve` in one terminal and `lp-cli dev <host>` in another, which is cumbersome.

The goal is to make `lp-cli dev` run an in-memory server by default when no host is specified, using a local transport for communication. This should minimize duplication with the existing `serve` command.

## Current State

- `dev` command: Requires mandatory `host` parameter, connects via WebSocket
- `serve` command: Starts server with filesystem initialization, LpServer creation, WebSocket transport, and synchronous server loop
- `LocalTransport` (in `lp-shared`): Single-threaded, `Rc<RefCell>`, designed for tests
- `WebSocketServerTransport`: Uses tokio runtime internally but provides sync polling interface
- Server loop: Synchronous, blocks on `transport.receive()` polling

## Questions

### Q1: Async Local Transport Implementation

**Context**: The existing `LocalTransport` in `lp-shared` is single-threaded and uses `Rc<RefCell>`, which is fine for tests but not suitable for an async server loop running in a tokio runtime.

**Question**: How should we implement the async-capable local transport?

**Answer**: Use tokio channels (`mpsc::UnboundedSender/Receiver`) similar to `WebSocketServerTransport`. Create a new `AsyncLocalTransport` in `lp-app/apps/lp-cli/src/transport/local.rs` that uses tokio channels for communication. This is consistent with the existing `WebSocketServerTransport` pattern and works well with tokio's async runtime. The transport will use a pair of channels: one for client->server messages and one for server->client messages.

### Q2: Server Execution Model

**Context**: The server loop (`run_server_loop`) is currently synchronous and blocks. When `dev` runs an in-memory server, we need to run both server and client in the same process.

**Question**: How should we execute the server when running in-memory?

**Answer**: Use a single tokio runtime with an async server loop. Create a tokio runtime at the start of the `dev` command (or use `runtime.block_on()`). Convert `run_server_loop` to `async fn run_server_loop_async()` and spawn it as a tokio task using `tokio::spawn()`. Use `tokio::time::sleep()` instead of `std::thread::sleep()` for the polling delay. This approach is consistent with how `WebSocketServerTransport` works and provides better resource management and graceful shutdown capabilities.

### Q3: Host Parameter Handling

**Context**: Currently `host` is a mandatory `String` parameter in the CLI.

**Question**: How should we handle the `host` parameter to make it optional?

**Answer**: Add a `HostSpecifier::Local` variant. Change `host` to `Option<String>` in `DevArgs` and CLI definition. When `None`, parse as `HostSpecifier::Local`. When `Some(url)`, parse using existing `HostSpecifier::parse()`. This is more explicit and consistent with the existing `HostSpecifier` enum pattern. Update `HostSpecifier::parse()` to handle the local case (either when string is empty/None, or add explicit parsing for "local").

### Q4: Server Lifecycle Management

**Context**: When `dev` starts an in-memory server, we need to manage its lifecycle.

**Question**: When should the in-memory server shut down?

**Answer**: Keep both server and client running in loops until Ctrl+C. The `dev` command should:
1. Start the in-memory server in a tokio task (server loop)
2. Push/load the project initially
3. Enter a client loop that processes messages (similar to how `serve` runs continuously)
4. Handle Ctrl+C signal to gracefully shut down both server and client
5. Use `tokio::signal::ctrl_c()` or `tokio::select!` to handle shutdown signal

This matches the `serve` command behavior but with both server and client in the same process. The server processes requests while the client can watch for changes or handle other operations.

### Q5: Multiple Client Connections

**Context**: The current server loop handles multiple connections (via `WebSocketServerTransport`).

**Question**: Should the in-memory server support multiple clients, or is one client per server sufficient?

**Answer**: Single client only. For the development use case, one client per server is sufficient. The local transport will be simpler - just a direct pair of transports (client and server) without connection management or connection IDs. This simplifies the implementation while meeting the development workflow needs.

### Q6: Server Initialization Refactoring

**Context**: The `serve` command has initialization logic that we want to reuse: `initialize_server`, `create_filesystem`, `LpServer::new`.

**Question**: How should we refactor the server initialization code to minimize duplication?

**Answer**: Extract server creation logic into a top-level `server` module at `lp-app/apps/lp-cli/src/server.rs` (or `server/mod.rs` if it grows). Create functions like `pub fn create_server(dir: Option<&Path>, memory: bool, init: bool) -> Result<(LpServer, Box<dyn LpFs>)>` and potentially `pub async fn run_server_loop_async(...)`. Both `serve` and `dev` commands can use these functions. This keeps the shared server code at the top level rather than nested under commands.

### Q7: Server Configuration

**Context**: The `serve` command requires `server.json` configuration (or `--init` flag to create it).

**Question**: Should the in-memory server require `server.json`, or should it use default configuration?

**Answer**: Use default `ServerConfig` without creating any file. Since it's an in-memory filesystem, the user wouldn't interact with `server.json` either way. Simply use `ServerConfig::default()` when creating the server. No need to create a file in the in-memory filesystem.

## Notes

- The existing `LocalTransport` in `lp-shared` should remain unchanged for tests
- Use tokio channels for async local transport (consistent with WebSocketServerTransport pattern)
- Convert server loop to async function for tokio compatibility
- Use tokio runtime for both server and client operations in `dev` command
- Consider error handling when server task panics or encounters errors
- Use `tokio::select!` or `CancellationToken` for graceful shutdown