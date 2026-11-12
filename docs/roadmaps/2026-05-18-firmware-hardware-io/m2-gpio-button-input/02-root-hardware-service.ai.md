# Phase 2: Root Hardware Service

## Scope Of Phase

In scope:

- Make firmware/app root own a single hardware registry for the device.
- Pass that registry into hardware-facing providers instead of letting each
  provider create a private registry.
- Keep projects and nodes out of hardware ownership.
- Preserve current output behavior.

Out of scope:

- Button implementation.
- Public hardware introspection API.
- Changing authored output config from numeric `pin` to string addresses.
- Multi-threaded hardware ownership.

## Code Organization Reminders

- Keep root hardware wiring in firmware/root service modules.
- Keep output providers focused on protocol output, not board policy.
- Avoid putting hardware ownership in nodes.
- Tests belong at the bottom of the files they exercise.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

1. Share the registry with `Rc<HardwareRegistry>`.

   `HardwareRegistry` already uses `RefCell` internally and can be shared by
   immutable reference. Do not wrap it in `RefCell` unless a concrete borrow
   problem appears.

2. Add constructor variants:

   - `MemoryOutputProvider::with_hardware_registry(registry: Rc<HardwareRegistry>)`
   - `SyscallOutputProvider::new_with_hardware_registry(registry: Rc<HardwareRegistry>)`
   - `Esp32OutputProvider::new(registry: Rc<HardwareRegistry>)`

   Keep existing `new()` convenience constructors for tests where appropriate,
   but production firmware should not use a private provider-owned registry.

3. Update provider structs:

   - `hardware_registry: HardwareRegistry` becomes
     `hardware_registry: Rc<HardwareRegistry>`.
   - Existing calls such as `self.hardware_registry.ensure_capability(...)`
     should continue to work through `Rc` deref.

4. Update `lp-fw/fw-esp32/src/main.rs`:

   - After Phase 1 loads the selected `HardwareManifest`, create:

     ```rust
     let hardware_registry = Rc::new(HardwareRegistry::new(hardware_manifest));
     ```

   - Construct `Esp32OutputProvider::new(Rc::clone(&hardware_registry))`.
   - Keep `hardware_registry` in root scope for Phase 4 button construction and
     future radio service construction.

5. Update `lp-fw/fw-emu/src/main.rs`:

   - Create one `Rc<HardwareRegistry>` at emu root.
   - Pass it into `SyscallOutputProvider`.

6. Do not add `HardwareRegistry` to project/node structs in this phase.

   `LpServer` may continue to receive the already-constructed output provider.
   Hardware remains firmware-root owned, like transports.

## Tests To Add Or Update

- Existing output provider tests should still pass.
- Add a focused lpc-shared test proving two providers/helpers sharing one
  registry see each other's claims if a simple test helper is available.
- Update any test constructors impacted by signature changes.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-shared output
cargo test -p lpc-engine engine_services
cargo check -p lpa-server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

