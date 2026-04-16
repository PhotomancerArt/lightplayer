# Phase 2: Create LocalServerTransport

## Description

Create `LocalServerTransport` struct that encapsulates the in-memory server thread lifecycle and provides a `ClientTransport` interface. This abstracts away the server creation and thread management from the handler.

## Tasks

1. Create `lp-app/apps/lp-cli/src/client/local_server.rs`:
   - Define `LocalServerTransport` struct with:
     - `server_handle: JoinHandle<()>`
     - `client_transport: Box<dyn ClientTransport + Send>`
     - `closed: bool`
   - Implement `new() -> Result<Self, Error>`:
     - Create transport pair using `create_local_transport_pair()`
     - Spawn server thread with `run_server_loop_async()`
     - Return struct with client transport
   - Implement `client_transport(&self) -> &dyn ClientTransport`
   - Implement `close(mut self) -> Result<(), Error>`:
     - Set `closed = true`
     - Drop client transport (signals shutdown to server)
     - Wait for server thread to finish
   - Implement `Drop` that calls `close()` if not already called

2. Update `lp-app/apps/lp-cli/src/client/mod.rs`:
   - Add `pub mod local_server;`
   - Re-export `LocalServerTransport`

3. Add basic test in `local_server.rs`:
   - Test server spawns correctly
   - Test `client_transport()` returns working transport
   - Test sending/receiving messages through local server
   - Test `close()` stops server

## Success Criteria

- `LocalServerTransport` struct exists and compiles
- `new()` spawns server thread successfully
- `client_transport()` returns working transport
- `close()` stops server and waits for thread
- `Drop` calls `close()` if not already called
- Basic test passes
- Code compiles without errors

## Implementation Notes

- Use `std::thread::Builder::new().name("lp-server")` for server thread
- Server thread should create its own tokio runtime
- Use `create_local_transport_pair()` from `client/local.rs`
- Server thread should run `run_server_loop_async()` until transport closes
- `close()` should be `mut self` to take ownership and ensure single close
- Consider using `Arc<AtomicBool>` for closed state if needed for `Drop`
