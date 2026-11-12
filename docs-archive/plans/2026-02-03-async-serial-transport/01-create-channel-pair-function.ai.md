# Phase 1: Create Channel Pair Function

## Scope of Phase

Create a helper function `create_emulator_serial_transport_pair()` that creates the channel pairs needed for emulator communication. This function will be used by the transport factory but doesn't spawn the thread yet - that comes in the next phase.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

1. Create `lp-core/lp-client/src/transport_serial/mod.rs`:
   - Add module declaration
   - Re-export `AsyncSerialClientTransport` (will be created in phase 3)
   - Re-export `create_emulator_serial_transport_pair` (will be created in phase 2)

2. Create `lp-core/lp-client/src/transport_serial/emulator.rs`:
   - Add `create_emulator_serial_transport_pair()` function:
     ```rust
     pub fn create_emulator_serial_transport_pair(
         emulator: Arc<Mutex<Riscv32Emulator>>,
     ) -> Result<(
         mpsc::UnboundedSender<ClientMessage>,
         mpsc::UnboundedReceiver<ServerMessage>,
         oneshot::Sender<()>,
     ), TransportError>
     ```
   - Creates three channels:
     - `client_tx/rx`: Client messages (client -> emulator thread)
     - `server_tx/rx`: Server messages (emulator thread -> client)
     - `shutdown_tx/rx`: Shutdown signal (client -> emulator thread)
   - Returns the client-side ends: `client_tx`, `server_rx`, `shutdown_tx`
   - The emulator thread will receive the other ends (to be passed in phase 2)

3. Add necessary imports:
   - `tokio::sync::mpsc`
   - `tokio::sync::oneshot`
   - `lp_model::{ClientMessage, ServerMessage, TransportError}`
   - `lp_riscv_emu::Riscv32Emulator`
   - `std::sync::{Arc, Mutex}`

4. Update `lp-core/lp-client/src/lib.rs`:
   - Add `pub mod transport_serial;` (if not already present)

## Tests

Add a test in `lp-core/lp-client/src/transport_serial/emulator.rs`:
- Test that `create_emulator_serial_transport_pair()` creates channels successfully
- Verify channels are connected (send on one end, receive on other)

## Validate

Run: `cd lp-core/lp-client && cargo test transport_serial`

Fix any warnings or errors. Keep code compiling.
