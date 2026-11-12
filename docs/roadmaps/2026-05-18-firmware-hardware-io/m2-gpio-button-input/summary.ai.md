# M2 GPIO Button Input And Root Hardware Ownership Summary

## What Was Built

- Added a compiled ESP32-C6 default hardware manifest sourced from
  `lp-core/lpc-shared/boards/seeed/xiao-esp32-c6.toml`.
- Added ESP32 startup loading for `/hardware.toml`, with non-fatal fallback to
  the compiled default when the file is missing or invalid.
- Moved ESP32 firmware startup to mount the filesystem before constructing
  hardware-facing providers.
- Refactored output providers to accept a shared root-owned
  `Rc<HardwareRegistry>`.
- Wired normal ESP32 firmware and `fw-emu` to create one device-level hardware
  registry and pass it into output providers.
- Removed the old hand-coded provisional ESP32 manifest.
- Added shared button event and debounce types in `lpc-shared::hardware`.
- Added a virtual button claimant and tests proving output/button conflicts use
  the same registry.
- Added ESP32 GPIO4 button input with internal pull-up and a `test_button`
  firmware diagnostic mode.

## Decisions For Future Reference

#### Firmware-Root Hardware Ownership

- **Decision:** Firmware root owns the hardware registry, not nodes, projects,
  or individual output/button providers.
- **Why:** Hardware is a once-per-device service similar to transports, and all
  hardware users need shared resource arbitration.
- **Rejected alternatives:** Private provider registries; node-owned hardware
  claims.
- **Revisit when:** The server exposes hardware introspection or active-claim
  editing to clients.

#### Default Plus `/hardware.toml`

- **Decision:** Firmware compiles in a default manifest and optionally loads
  `/hardware.toml` from the root filesystem at startup.
- **Why:** Devices remain recoverable with a known-good default while allowing
  calibrated board policy later.
- **Rejected alternatives:** Rust-only hardcoded manifest; project-local
  manifest; fail boot on invalid override.
- **Revisit when:** Runtime hardware manifest editing or reload semantics are
  added.

#### First Button Slice Uses GPIO4

- **Decision:** The first ESP32 button diagnostic uses owned `GPIO4` with
  internal pull-up and active-low button-to-ground wiring.
- **Why:** GPIO4 is already returned by board init, so M2 can prove the button
  path without taking on arbitrary HAL pin dispatch.
- **Rejected alternatives:** Dynamic GPIO dispatch in this milestone.
- **Revisit when:** Input/output pin selection becomes user-facing.

## Validation

- `cargo fmt --check`
- `cargo test -p lpc-shared hardware`
- `cargo test -p lpc-shared output`
- `cargo check -p lpc-shared --no-default-features`
- `cargo test -p lpc-engine engine_services`
- `cargo check -p lpa-server`
- `cargo test -p lpa-server --no-run`
- `cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,test_button`
- `cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu`

Note: both `fw-tests` tests are currently ignored with the existing
`awaits_canonical_project_sync` messages, but the command passed.

## Deferred

- Dynamic ESP32 GPIO dispatch for output and button providers.
- Runtime editing or hot-reload of `/hardware.toml`.
- Server/client hardware introspection.
- M3 ESP-NOW radio service using the same root-owned registry.
