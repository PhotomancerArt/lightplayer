# Phase 4: Convert Server Loop to Async

## Description

Add an async version of the server loop that works with tokio runtime. This allows the server to run as a tokio task alongside the client in the same runtime.

## Tasks

1. Update `lp-app/apps/lp-cli/src/server.rs`:
   - Add `run_server_loop_async()` function:
     ```rust
     pub async fn run_server_loop_async<T: ServerTransport>(
         mut server: LpServer,
         mut transport: T,
     ) -> Result<()>
     ```
   - Implementation:
     - Main loop: `loop { ... }`
     - Collect incoming messages (non-blocking):
       ```rust
       let mut incoming_messages = Vec::new();
       loop {
           match transport.receive() {
               Ok(Some(msg)) => incoming_messages.push(Message::Client(msg)),
               Ok(None) => break,
               Err(e) => {
                   eprintln!("Transport error: {}", e);
                   break;
               }
           }
       }
       ```
     - Process messages if any:
       ```rust
       if !incoming_messages.is_empty() {
           match server.tick(16, incoming_messages) {
               Ok(responses) => {
                   for response in responses {
                       if let Message::Server(server_msg) = response {
                           if let Err(e) = transport.send(server_msg) {
                               eprintln!("Failed to send response: {}", e);
                           }
                       }
                   }
               }
               Err(e) => {
                   eprintln!("Server error: {}", e);
               }
           }
       }
       ```
     - Use `tokio::time::sleep(Duration::from_millis(10)).await` instead of `std::thread::sleep()`

2. Keep existing `run_server_loop()` in `commands/serve/server_loop.rs` unchanged (for `serve` command)

3. Add tests:
   - Test async server loop with local transport
   - Test message processing and response sending
   - Test error handling in async context

## Success Criteria

- `run_server_loop_async()` function exists and is async
- Server loop processes messages correctly
- Uses `tokio::time::sleep()` for polling delay
- Existing sync server loop still works
- All tests pass
- Code compiles without warnings

## Implementation Notes

- Keep sync version for `serve` command (no breaking changes)
- Async version follows same logic as sync version, just uses async sleep
- Error handling should log errors but continue running (same as sync version)
- The function will run until transport error or cancellation
