# Phase 5: Emu And Conflict Tests

## Scope Of Phase

In scope:

- Wire `fw-emu` through the root-owned hardware registry pattern.
- Add virtual button support for tests/diagnostics.
- Add host tests for cross-consumer resource conflicts.
- Keep emulator output behavior working.

Out of scope:

- Real emulated GPIO input from host UI.
- Radio transport.
- Project graph event semantics.

## Code Organization Reminders

- Keep emu-specific syscall/logging code under `lp-fw/fw-emu/`.
- Keep generic virtual button helpers in `lpc-shared` only if they are useful
  outside a single test.
- Avoid test-only APIs leaking into production surfaces unless they are
  deliberately useful diagnostics.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

1. Update `fw-emu` startup:

   - Create one `Rc<HardwareRegistry>` from
     `HardwareManifest::virtual_single_rmt_gpio_board()`.
   - Pass it to `SyscallOutputProvider`.
   - Use the same registry for any virtual button diagnostic.

2. Add virtual button tests.

   Tests should prove the same root registry prevents conflicts across
   consumers:

   - button first, output same pin fails
   - output first, button same pin fails
   - output GPIO18 plus button GPIO4 succeeds
   - closing/dropping button releases the pin

3. Add or update engine/server tests only if constructor signatures require it.

   Existing `LpServer::new` call sites may continue to pass only an output
   provider. The root registry does not need to be added to `LpServer` unless
   implementation discovers a concrete need.

4. Add a tiny emu diagnostic if useful.

   It can log synthetic button events, but this is optional. Do not let this
   phase grow into host-driven GPIO simulation.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-shared hardware
cargo test -p lpc-shared output
cargo test -p lpc-engine engine_services
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
```

