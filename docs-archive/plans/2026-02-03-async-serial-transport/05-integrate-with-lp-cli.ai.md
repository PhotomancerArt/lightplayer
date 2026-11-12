# Phase 5: Integrate with lp-cli client_connect

## Scope of Phase

Wire up `HostSpecifier::Emulator` in `lp-cli`'s `client_connect()` function so that `--push emu` creates an `AsyncSerialClientTransport`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

1. Update `lp-cli/src/client/client_connect.rs`:

   Add case for `HostSpecifier::Emulator`:
   ```rust
   HostSpecifier::Emulator => {
       // Create emulator and load firmware
       // Use same pattern as test: build fw-emu, load ELF, create emulator
       // Then create transport pair
       let transport = create_emulator_serial_transport_pair(emulator)?;
       Ok(Box::new(transport))
   }
   ```

2. Implementation details:
   - Build fw-emu binary (use `lp_riscv_emu::test_util::ensure_binary_built()`)
   - Load ELF (use `lp_riscv_elf::load_elf()`)
   - Create `Riscv32Emulator` with `TimeMode::Real`
   - Set up stack pointer and PC (same as test)
   - Create `Arc<Mutex<Riscv32Emulator>>`
   - Call `create_emulator_serial_transport_pair()`

3. Add necessary imports:
   - `lp_client::transport_serial::emulator::create_emulator_serial_transport_pair`
   - `lp_riscv_emu::{test_util::BinaryBuildConfig, LogLevel, Riscv32Emulator, TimeMode}`
   - `lp_riscv_elf::load_elf`
   - `lp_riscv_inst::Gpr`
   - `std::sync::{Arc, Mutex}`
   - `std::fs::read`

4. Update help text in `lp-cli/src/main.rs`:
   - Update `--push` flag description to mention "emu" or "emulator" as options
   - Example: `"Push local project to server. Optionally specify remote host (e.g., ws://localhost:2812/, serial:auto, or emu)."`

5. Error handling:
   - Handle binary build failures
   - Handle ELF load failures
   - Handle emulator creation failures
   - Return appropriate `anyhow::Error` messages

## Tests

Add test in `lp-cli/src/client/client_connect.rs`:
- Test that `client_connect(HostSpecifier::Emulator)` creates transport successfully
- Test that transport can send/receive messages (basic smoke test)

Note: This test will need to build fw-emu, so it may be slow.

## Validate

Run: `cd lp-cli && cargo test client_connect`

Fix any warnings or errors. Keep code compiling.
