# Phase 6: Add Ctrl+C Handling and Client Loop

## Description

Implement continuous client loop and graceful shutdown handling via Ctrl+C. Both server and client should run continuously until the user presses Ctrl+C.

## Tasks

1. Update `lp-app/apps/lp-cli/src/commands/dev/handler.rs`:
   - Add `run_client_loop()` async function:
     ```rust
     async fn run_client_loop(
         client: &mut LpClient,
         transport: &mut dyn ClientTransport,
     ) -> Result<()>
     ```
   - Implementation:
     - Main loop: `loop { ... }`
     - Poll transport for messages: `transport.receive()`
     - Process messages via client (check `LpClient` API for message handling)
     - Use `tokio::time::sleep(Duration::from_millis(10)).await` for polling delay
     - Handle errors appropriately

2. Update `handle_dev()` function:
   - For `HostSpecifier::Local`:
     - After push/load project, use `tokio::select!`:
       ```rust
       tokio::select! {
           _ = tokio::signal::ctrl_c() => {
               println!("Shutting down...");
           }
           result = run_client_loop(&mut client, &mut client_transport) => {
               result?;
           }
       }
       ```
   - For `HostSpecifier::WebSocket`:
     - Similar `tokio::select!` pattern for WebSocket mode

3. Handle graceful shutdown:
   - When Ctrl+C is received, exit the select block
   - Server task will detect closed channel and exit naturally
   - Print shutdown message

4. Add tests:
   - Test client loop processes messages
   - Test Ctrl+C handling (may need to mock signal)
   - Test graceful shutdown

## Success Criteria

- Client loop runs continuously and processes messages
- Ctrl+C signal is handled gracefully
- Both server and client loops exit cleanly on Ctrl+C
- Server task detects closed channel and exits
- All tests pass
- Code compiles without warnings

## Implementation Notes

- Check `LpClient` API to see how to handle incoming messages
- Client loop may need to handle different message types
- Use `tokio::signal::ctrl_c()` for signal handling
- Server task will naturally exit when channels are closed (no explicit cancellation needed)
- Consider adding a small delay before exit to allow final messages to be processed
