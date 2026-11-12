# Phase 3: Create AsyncSerialClientTransport Struct

## Scope of Phase

Create the generic `AsyncSerialClientTransport` struct that implements `ClientTransport`. This transport is generic and doesn't know about emulator vs hardware - it just uses channels.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

1. Create `lp-core/lp-client/src/transport_serial.rs`:
   - Define `AsyncSerialClientTransport` struct:
     ```rust
     pub struct AsyncSerialClientTransport {
         client_tx: Option<mpsc::UnboundedSender<ClientMessage>>,
         server_rx: mpsc::UnboundedReceiver<ServerMessage>,
         shutdown_tx: Option<oneshot::Sender<()>>,
         thread_handle: Option<JoinHandle<()>>,
         closed: bool,
     }
     ```

2. Implement `new()` constructor (internal, used by factory functions):
   ```rust
   pub(crate) fn new(
       client_tx: mpsc::UnboundedSender<ClientMessage>,
       server_rx: mpsc::UnboundedReceiver<ServerMessage>,
       shutdown_tx: oneshot::Sender<()>,
       thread_handle: JoinHandle<()>,
   ) -> Self
   ```

3. Implement `ClientTransport` trait:
   - `send()`: Send via `client_tx` (returns `ConnectionLost` if closed or channel closed)
   - `receive()`: Receive from `server_rx.recv().await` (returns `ConnectionLost` if closed or channel closed)
   - `close()`: 
     - Send shutdown signal via `shutdown_tx`
     - Drop `client_tx` (closes channel, signals emulator thread)
     - Wait for thread handle to finish (with timeout, similar to `LocalServerTransport`)
     - Mark as closed

4. Implement `Drop` trait:
   - Call `close()` if not already closed (best-effort cleanup)

5. Add necessary imports:
   - `async_trait::async_trait`
   - `tokio::sync::{mpsc, oneshot}`
   - `std::thread::JoinHandle`
   - `std::time::{Duration, Instant}`
   - `lp_model::{ClientMessage, ServerMessage, TransportError}`
   - `crate::transport::ClientTransport`

6. Update `lp-core/lp-client/src/transport_serial/mod.rs`:
   - Re-export `AsyncSerialClientTransport`

7. Update `lp-core/lp-client/src/transport_serial/emulator.rs`:
   - Update `create_emulator_serial_transport_pair()` to return `AsyncSerialClientTransport`:
     ```rust
     pub fn create_emulator_serial_transport_pair(
         emulator: Arc<Mutex<Riscv32Emulator>>,
     ) -> Result<AsyncSerialClientTransport, TransportError>
     ```

## Tests

Add tests in `lp-core/lp-client/src/transport_serial.rs`:
- Test `send()` and `receive()` work correctly
- Test `close()` waits for thread and cleans up
- Test `Drop` implementation calls `close()`
- Test error handling when channels are closed

## Validate

Run: `cd lp-core/lp-client && cargo test transport_serial`

Fix any warnings or errors. Keep code compiling.
