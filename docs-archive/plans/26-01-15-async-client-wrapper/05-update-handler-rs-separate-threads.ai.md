# Phase 5: Update handler.rs for Separate Threads

## Description

Update `handle_dev_local` to spawn the server on a separate thread and use `AsyncLpClient` on the main thread.

## Tasks

1. Update `handle_dev_local()`:
   - Create server and transport pair
   - Spawn server on separate thread using `std::thread::spawn`:
     - Create tokio runtime in thread
     - Run `run_server_loop_async()` in that runtime
   - Create `AsyncLpClient` on main thread with client transport
   - Use `AsyncLpClient` methods for push/load operations
   - Update client loop to use async client

2. Update `run_client_loop()` to `run_client_loop_async()`:
   - Make it async
   - Use `AsyncLpClient` for message processing if needed
   - Or keep using transport directly but with async polling

3. Ensure proper thread coordination:
   - Server thread runs independently
   - Client thread uses async client wrapper
   - Both communicate via `Send` transport channels

## Success Criteria

- Server runs on separate thread successfully
- Client uses `AsyncLpClient` on main thread
- Push and load operations work correctly
- Client loop runs continuously
- Code compiles without errors

## Implementation Notes

- Use `std::thread::spawn` for server thread
- Create tokio runtime in server thread: `Runtime::new().unwrap()`
- Server thread should run until transport disconnects or error
- Main thread uses `Runtime::new()` and `block_on()` for async operations
- Ensure proper cleanup on shutdown
