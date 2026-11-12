# Phase 5: Update Dev Command for In-Memory Server

## Description

Modify the `dev` command to support local transport and in-memory server. When `host` is `None` or `Local`, create an in-memory server and use local transport. When `host` specifies a WebSocket URL, use existing WebSocket transport logic.

## Tasks

1. Update `lp-app/apps/lp-cli/src/main.rs`:
   - Change `Dev` variant: `host: String` -> `host: Option<String>`
   - Update argument parsing to make host optional

2. Update `lp-app/apps/lp-cli/src/commands/dev/args.rs`:
   - Change `host: String` to `host: Option<String>`

3. Update `lp-app/apps/lp-cli/src/commands/dev/handler.rs`:
   - Import necessary modules:
     - `crate::server::{create_server, run_server_loop_async}`
     - `crate::transport::local::{create_local_transport_pair, AsyncLocalClientTransport}`
     - `crate::transport::HostSpecifier`
     - `tokio::runtime::Runtime`
   - Update `handle_dev()` function:
     - Parse host specifier: `HostSpecifier::parse_optional(args.host.as_deref())?`
     - Match on `HostSpecifier`:
       - `HostSpecifier::Local`:
         - Create tokio runtime: `Runtime::new()?`
         - Call `runtime.block_on(async { ... })`:
           - Create server: `create_server(None, true, None)?`
           - Create transport pair: `create_local_transport_pair()`
           - Spawn server task: `tokio::spawn(run_server_loop_async(server, server_transport))`
           - Create client: `LpClient::new()`
           - Push project if `args.push` is true
           - Load project
           - Enter client loop (will be implemented in next phase)
       - `HostSpecifier::WebSocket { url }`:
         - Use existing WebSocket transport logic
         - Create client and push/load project
         - Enter client loop (will be implemented in next phase)

4. Add tests:
   - Test `dev` command with `host = None` (should use local transport)
   - Test `dev` command with `host = Some("local")` (should use local transport)
   - Test `dev` command with `host = Some("ws://...")` (should use WebSocket transport)
   - Test server starts and processes messages in local mode

## Success Criteria

- `dev` command accepts optional host parameter
- When host is `None` or `"local"`, in-memory server starts
- Local transport is created and connected
- Server task spawns and runs
- Client can push and load project via local transport
- WebSocket mode still works as before
- All tests pass
- Code compiles without warnings

## Implementation Notes

- For now, client loop can be a placeholder that exits immediately (will be implemented in next phase)
- Server task should be spawned before client operations to ensure it's ready
- Handle errors appropriately - if server creation fails, don't start anything
- Keep WebSocket path unchanged to avoid breaking existing functionality
