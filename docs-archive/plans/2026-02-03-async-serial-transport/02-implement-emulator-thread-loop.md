# Phase 2: Implement Emulator Thread Loop

## Scope of Phase

Implement the emulator thread loop that runs continuously, processing messages and communicating via serial I/O. This is the core of the emulator integration.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

1. Update `lp-core/lp-client/src/transport_serial/emulator.rs`:

   Add `emulator_thread_loop()` function:
   ```rust
   fn emulator_thread_loop(
       emulator: Arc<Mutex<Riscv32Emulator>>,
       mut client_rx: mpsc::UnboundedReceiver<ClientMessage>,
       server_tx: mpsc::UnboundedSender<ServerMessage>,
       mut shutdown_rx: oneshot::Receiver<()>,
   )
   ```

   The loop should:
   - Check for shutdown signal (non-blocking via `try_recv()`)
   - Process incoming client messages (non-blocking via `try_recv()`)
   - For each client message:
     - Serialize to JSON
     - Write to emulator serial input via `emulator.serial_write()`
   - Run emulator until yield: `run_until_yield(MAX_STEPS)` (e.g., 100_000_000)
   - Drain serial output: `emulator.drain_serial_output()`
   - Parse messages from serial output (newline-terminated JSON)
   - Send parsed `ServerMessage` via `server_tx`
   - Repeat

2. Message parsing logic:
   - Buffer partial reads (similar to `SerialEmuClientTransport::read_message()`)
   - Look for newline-terminated JSON messages
   - Parse each complete message with `serde_json::from_str::<ServerMessage>()`
   - Handle parse errors gracefully (log and continue)

3. Error handling:
   - If `run_until_yield()` fails, log error and close `server_tx` (drops sender, signals connection lost)
   - If serialization fails, log and continue (don't crash thread)
   - If channel send fails, log and continue (channel may be closed)

4. Update `create_emulator_serial_transport_pair()`:
   - Now spawns the thread:
     ```rust
     let thread_handle = std::thread::Builder::new()
         .name("lp-emulator-serial".to_string())
         .spawn(move || {
             emulator_thread_loop(emulator, client_rx, server_tx, shutdown_rx);
         })?;
     ```
   - Returns thread handle along with channels

5. Constants:
   - `const MAX_STEPS_PER_ITERATION: u64 = 100_000_000;`

6. Add necessary imports:
   - `serde_json`
   - `log`
   - `std::thread`

## Tests

Add tests in `lp-core/lp-client/src/transport_serial/emulator.rs`:
- Test that thread spawns successfully
- Test that client messages are written to emulator serial
- Test that server messages from emulator serial are sent via channel
- Test shutdown signal stops the thread

Note: These tests will need a mock emulator or test emulator setup.

## Validate

Run: `cd lp-core/lp-client && cargo test transport_serial`

Fix any warnings or errors. Keep code compiling.
