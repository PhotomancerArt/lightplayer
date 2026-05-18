# M2 GPIO Button Input And Root Hardware Ownership Design

## Scope

This milestone creates a root-owned hardware service and builds GPIO button
input on top of it.

It must preserve the current shader compile/execute path and keep firmware
validation focused on IO ownership, manifest loading, and button diagnostics.

## File Structure

```text
lp-core/lpc-shared/src/hardware/
  mod.rs
  default_manifests.rs          # compiled default board manifests
  hardware_registry.rs          # unchanged core registry, shared through Rc
  hardware_manifest_file.rs     # existing TOML parse/lower path
  button_event.rs               # small shared button event/debounce vocabulary
  virtual_button.rs             # optional no_std test/emu helper

lp-core/lpc-shared/src/output/
  memory.rs                     # accepts root-owned Rc<HardwareRegistry>

lp-fw/fw-esp32/src/
  hardware/
    mod.rs
    manifest_loader.rs          # /hardware.toml override plus default fallback
    root_hardware.rs            # root device hardware service wiring
    button.rs                   # ESP32 GPIO button with pull-up and debounce
  output/provider.rs            # accepts root-owned Rc<HardwareRegistry>
  main.rs                       # creates hardware once after FS mount

lp-fw/fw-emu/src/
  hardware.rs                   # emu root hardware service if needed
  output.rs                     # accepts root-owned Rc<HardwareRegistry>

docs/roadmaps/2026-05-18-firmware-hardware-io/m2-gpio-button-input/
  00-notes.md
  00-design.md
  01-default-and-override-manifests.md
  02-root-hardware-service.md
  03-button-event-and-debounce-core.md
  04-esp32-button-diagnostic.md
  05-emu-and-conflict-tests.md
  06-cleanup-validation-summary.md
```

Names can be adjusted to match local module conventions during implementation,
but keep the one-concept-per-file shape.

## Architecture Summary

Firmware startup owns hardware as a once-per-device service:

```text
flash/in-memory filesystem
        |
        v
load /hardware.toml or compiled default manifest
        |
        v
Rc<HardwareRegistry>
        |
        +--> Esp32OutputProvider claims GPIO + RMT for WS281x
        |
        +--> Esp32ButtonInput claims GPIO input for button
        |
        +--> future ESP-NOW radio service claims radio resource
```

The important ownership boundary is that nodes and projects do not own
hardware. They request behavior through engine/server services, and those
services talk to root-owned providers.

For this milestone, `LpServer` may continue receiving an output provider rather
than a full hardware object. The firmware root creates the registry, constructs
the output provider and button service with clones of that registry handle, and
then starts the server loop.

## Main Components

### Default Manifest

Add a shared compiled default manifest function backed by the checked-in TOML.
The first default should use `seeed/xiao-esp32-c6.toml`.

The function should parse through `HardwareManifestFile::read_toml(...).to_manifest()`
so default and override manifests use identical validation/lowering behavior.

### Startup Override

Add an ESP32 firmware manifest loader that tries to read `/hardware.toml` from
the mounted `LpFs`.

Behavior:

- Missing file: use compiled default.
- Parse or validation failure: log warning and use compiled default.
- Valid file: use the override manifest and log its board id/product.

Mount the filesystem before constructing hardware providers so the override can
affect the root registry.

### Root Hardware Service

Use `Rc<HardwareRegistry>` as the shared root handle for this milestone.

Providers should accept the shared registry:

- `Esp32OutputProvider::new(registry: Rc<HardwareRegistry>)`
- `MemoryOutputProvider::with_hardware_registry(registry: Rc<HardwareRegistry>)`
- `SyscallOutputProvider::new_with_hardware_registry(registry: Rc<HardwareRegistry>)`

Keep convenience constructors that create a virtual/default registry for tests
where useful, but production firmware should use the root-owned registry.

### Button Event And Debounce Core

Add small shared types that are radio-friendly:

- `ButtonEventKind::Pressed | Released`
- `ButtonEvent { source, sequence, kind }`
- `ButtonDebouncer` for active-low button-to-ground wiring

Debouncer behavior should be deterministic and unit-testable without firmware
timers. It should accept raw pressed/not-pressed samples plus elapsed time or
timestamps and emit state changes only after a stable interval.

### ESP32 Button Input

Add an ESP32 button module that:

- Claims the GPIO through the root registry with `HardwareCapability::GpioInput`.
- Configures internal pull-up.
- Treats low as pressed.
- Runs debounce in a small poll method or diagnostic loop.
- Releases the lease when dropped if the type owns a lease.

The first firmware diagnostic may use GPIO4 only. Dynamic pin dispatch should be
left for future work.

### Emu And Conflict Tests

Add host/no_std-friendly tests that use one registry shared by output and button
helpers:

- output on GPIO4 then button on GPIO4 fails
- button on GPIO4 then output on GPIO4 fails
- output on GPIO18 and button on GPIO4 can coexist
- reserved GPIO cannot be claimed for button

This is the core proof that hardware is root-owned and not per-provider.

