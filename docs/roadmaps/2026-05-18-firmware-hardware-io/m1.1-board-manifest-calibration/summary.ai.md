# M1.1 Summary

Implemented the first pass of board manifest management for hardware profiles.

## Shipped

- Added a TOML-backed `HardwareManifestFile` model in `lpc-shared`.
- Added `HardwareTarget` with the initial supported targets: `esp32c6` and `rv32imac_emu`.
- Extended `HardwareManifest` with target, vendor, product, description, and URL metadata.
- Added checked-in board manifest storage under `lp-core/lpc-shared/boards/`.
- Added the first board manifest: `seeed/xiao-esp32-c6`.
- Added `lp-cli hardware` / `lp-cli hardware manifest` as developer-facing manifest CRUD:
  - interactive manifest browser when attached to a terminal
  - non-interactive list fallback
  - `list`, `show`, `validate`, `new`, `set`, and `delete` subcommands
- Updated repository docs to clarify that `lp-cli` is currently a developer-facing repo tool.

## Deferred

- Firmware-assisted calibration mode is still a planned stub.
- The physical pin scan loop still needs firmware test-mode support, reset handling, crash recording,
  and the enter/back/confirm workflow described in the notes.
- The first Seeed profile is a seed manifest; its labels and reserved pins should be replaced with
  measured data once calibration exists.

## Validation

- `cargo fmt --check`
- `cargo test -p lpc-shared hardware`
- `cargo check -p lpc-shared --no-default-features`
- `cargo test -p lp-cli hardware`
- `cargo check -p lp-cli`
- `cargo run -p lp-cli -- hardware manifest list`
- `cargo run -p lp-cli -- hardware manifest show seeed/xiao-esp32-c6`
- `cargo run -p lp-cli -- hardware manifest validate`
- `cargo run -p lp-cli -- hardware manifest`
