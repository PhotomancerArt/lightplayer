# Phase 6: Create Async Test (scene_render_emu_async.rs)

## Scope of Phase

Create an async test similar to `scene_render_emu.rs` but using the async serial transport with emulator running continuously on a separate thread.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

1. Create `lp-core/lp-client/tests/scene_render_emu_async.rs`:

   Base structure similar to `scene_render_emu.rs`:
   ```rust
   #[tokio::test]
   #[test_log::test]
   async fn test_scene_render_fw_emu_async() {
       // Arrange: Build fw-emu, create emulator, create async transport
       // Act: Send project files, load project, sync client view
       // Assert: Render frames and verify output
   }
   ```

2. Key differences from sync version:
   - Use `create_emulator_serial_transport_pair()` instead of `SerialEmuClientTransport::new()`
   - Use `TimeMode::Real` instead of `TimeMode::Simulated(0)`
   - Don't manually advance time - let real time advance naturally
   - Use `tokio::time::sleep()` to wait between frames instead of `advance_time()`
   - All client operations are async (already using `.await`)

3. Test structure:
   - Build fw-emu binary
   - Load ELF and create emulator with `TimeMode::Real`
   - Create async transport via `create_emulator_serial_transport_pair()`
   - Create `LpClient` with transport
   - Create project using `ProjectBuilder`
   - Write project files to firmware filesystem
   - Load project
   - Sync client view (initial sync)
   - Wait for real time to advance (sleep ~16ms for 60 FPS)
   - Sync client view again
   - Verify output data matches expected values
   - Repeat for multiple frames

4. Helper functions (reuse from sync test):
   - `collect_project_files()` - same as sync version
   - `sync_client_view()` - same as sync version (already async)
   - `assert_output_red()` - same as sync version

5. Time handling:
   - Use `tokio::time::sleep(Duration::from_millis(16))` between frames
   - This simulates ~60 FPS frame timing
   - Real time will advance naturally in the emulator

6. Add necessary imports:
   - `lp_client::transport_serial::emulator::create_emulator_serial_transport_pair`
   - `tokio::time::sleep`
   - `std::time::Duration`
   - All other imports from sync test

## Tests

The test itself is the validation. It should:
- Complete successfully
- Verify output data matches expected values for multiple frames
- Demonstrate that async transport works with continuous emulator

## Validate

Run: `cd lp-core/lp-client && cargo test scene_render_emu_async`

Fix any warnings or errors. Keep code compiling.

Note: This test will need to build fw-emu, so it may be slow.
