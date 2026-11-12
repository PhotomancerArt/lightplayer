# M2 GPIO Button Input And Root Hardware Ownership Notes

## Scope

This plan covers Milestone 2 of the firmware hardware IO roadmap plus the
manifest-loading work that should happen before button support:

- Compile a default board hardware manifest into firmware from the checked-in
  manifest data.
- At firmware startup, load `/hardware.toml` from the device filesystem when it
  exists and parses; otherwise use the compiled default.
- Refactor hardware ownership so the firmware/app root owns one hardware
  registry for the device, similar in spirit to transports.
- Pass that root-owned hardware service into outputs, GPIO buttons, and later
  radio services.
- Add first GPIO button support using internal pull-up and button-to-ground
  wiring.
- Add emulated button support and tests proving output/button resource conflicts
  use the same registry.

Out of scope:

- Full dynamic ESP32 LED pin dispatch. The RMT LED driver may still be
  GPIO18-only after this milestone, but it must consume the root-owned registry.
- LightPlayer project graph event semantics, playlists, or visual switching.
- ESP-NOW radio promotion. M3 builds on the button event shape.
- A production persistence/editing UX for `/hardware.toml`.

## Current State

- `HardwareManifestFile` parses TOML and lowers to runtime
  `HardwareManifest` in `lp-core/lpc-shared/src/hardware/hardware_manifest_file.rs`.
- Checked-in board manifests live under `lp-core/lpc-shared/boards/`.
- The current edited seed manifest is
  `lp-core/lpc-shared/boards/seeed/xiao-esp32-c6.toml`.
- ESP32 firmware still uses a separate hand-coded provisional manifest in
  `lp-fw/fw-esp32/src/board/esp32c6/hardware_manifest.rs`.
- `Esp32OutputProvider::new()` constructs its own private `HardwareRegistry`
  from that provisional manifest.
- `MemoryOutputProvider` and `fw-emu::SyscallOutputProvider` also own private
  registries.
- Private registries are enough for duplicate outputs inside one provider, but
  they cannot catch conflicts between different hardware users, such as a
  button and an output on the same GPIO.
- Firmware mounts the flash filesystem in `lp-fw/fw-esp32/src/main.rs` after it
  creates the output provider. This ordering must change so `/hardware.toml` can
  be read before constructing the root hardware registry and providers.
- `fw-esp32/src/board/esp32c6/init.rs` currently returns concrete GPIO18 and
  GPIO4 handles. GPIO18 is used for RMT LED output. GPIO4 is available for a
  first hardcoded diagnostic button path.
- `fw-esp32/src/tests/test_gpio_calibrate.rs` already uses `AnyPin::steal` in a
  calibration-only test mode. Production button support should prefer owned HAL
  pins returned by board init for the first slice.

## User Notes

- The default compiled manifest is fine for now.
- Startup should load `hardware.toml` or similar if it is available.
- Hardware ownership should be at the app/firmware root, not individual nodes.
- More precisely, hardware should be owned by the firmware itself, similar to
  transports. It is a once-per-device service.
- Nodes and projects should not own hardware. They ask services/providers for
  output/input behavior.
- The on-device GLSL compiler path remains non-negotiable and must not be gated
  or disturbed.

## Open Questions And Suggested Answers

### Where does the override file live?

Suggested answer: use `/hardware.toml` at the firmware filesystem root, next to
the `projects/` directory rather than inside a project.

Why: hardware is device-local, not project-local. This also lets all projects on
the same device see the same board policy and resource conflicts.

### What happens when `/hardware.toml` is invalid?

Suggested answer: log a warning and fall back to the compiled default manifest.
Do not fail boot.

Why: a broken hardware file should not brick the device or block USB recovery.
The warning should include the parse/validation error.

### Should `LpServer` own hardware directly?

Suggested answer: not in this milestone. Firmware root should own
`Rc<HardwareRegistry>` and inject it into hardware-facing providers/services.
`LpServer` can continue to own projects and output plumbing. If later app roots
need to expose hardware introspection over the wire, add a server-level service
field then.

Why: this matches the user's "firmware itself, like transports" clarification
and avoids making project/node lifecycle responsible for device resources.

### How should shared ownership work in no_std?

Suggested answer: use `Rc<HardwareRegistry>` where all consumers run in the
single-threaded firmware/server context. `HardwareRegistry` already uses
interior mutability through `RefCell`.

Why: this matches existing `Rc<RefCell<dyn OutputProvider>>` style and avoids
threading `&mut` root state through every tick path.

### What is the first real ESP32 button pin?

Suggested answer: use GPIO4 for the first diagnostic path because board init
already returns a concrete `GPIO4` handle and existing demos mention GPIO4 as a
known accessible pin. The manifest should still decide whether `/gpio/4`
supports `gpio-input` and is not reserved.

Why: dynamic HAL pin dispatch is larger than the button event/debounce slice.

