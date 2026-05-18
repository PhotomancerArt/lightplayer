# M1.1 Board Manifest Calibration Design

## Scope Of Work

M1.1 starts with a developer-facing board manifest management workflow in `lp-cli`, then layers the
physical calibration loop on top. The first useful slice should create and validate checked-in board
profile files from the repo before any firmware pulsing is required.

In scope:

- Add target/vendor/product metadata to the hardware manifest model and file schema.
- Define a checked-in board manifest directory.
- Add `lp-cli hardware manifest` CRUD/validation commands.
- Seed the first board profile for Seeed XIAO ESP32-C6.
- Update docs to state that `lp-cli` is developer-facing and repository-oriented.
- Plan, but do not necessarily complete in the first phase, the firmware-assisted GPIO calibration
  loop.

Out of scope:

- Splitting `lp-cli` into multiple binaries.
- User-facing packaged hardware manager UI.
- Making firmware consume TOML board files directly.
- Full dynamic ESP32 GPIO output dispatch.

## File Structure

```text
lp-core/lpc-shared/
  boards/
    seeed/
      xiao-esp32-c6.toml
  src/hardware/
    hardware_manifest.rs
    hardware_manifest_file.rs
    mod.rs

lp-cli/src/
  main.rs
  commands/
    mod.rs
    hardware/
      mod.rs
      args.rs
      handler.rs
      manifest/
        mod.rs
        board_manifest_store.rs
        board_manifest_commands.rs
        board_manifest_edit.rs
      calibrate/
        mod.rs
        notes.md or stub module only if needed

README.md
docs/architecture.md
docs/roadmaps/2026-05-18-firmware-hardware-io/m1.1-board-manifest-calibration/
```

## Architecture Summary

Checked-in TOML board profiles are the source artifacts. They live under
`lp-core/lpc-shared/boards/<vendor>/<product-id>.toml` so hardware data is shared by firmware,
emulator, server tests, and developer tooling.

`lpc_shared::hardware` should expose a serde-friendly board manifest file shape. This file shape can
convert to runtime `HardwareManifest`, but it may include authoring-only fields such as notes,
calibration status, or crash-suspect pins later. Runtime resource identity remains address based:
`"/gpio/18"` means HAL GPIO18.

The manifest target should be enum-like and intentionally small at first:

- `esp32c6` for ESP32-C6 firmware/HAL resources.
- `rv32imac_emu` for the RISC-V emulator's virtual hardware profile.

Tooling should validate that a selected manifest target matches the requested calibration or runtime
target before using GPIO/resource definitions.

`lp-cli hardware manifest ...` operates on the board directory after finding the repo root. It should
support:

- no subcommand: interactive picker/workflow
- `list`
- `show <id>`
- `new --target ... --vendor ... --product ... --url ... [--description ...]`
- `validate [id]`
- `delete <id>` with confirmation
- `set <id> --field value` or narrowly scoped edit commands if simple enough

The first board profile is:

```toml
id = "seeed/xiao-esp32-c6"
target = "esp32c6"
vendor = "seeed"
product = "XIAO ESP32-C6"
description = "Seeed Studio XIAO ESP32-C6 board profile."
url = "https://www.seeedstudio.com/Seeed-Studio-XIAO-ESP32C6-p-5884.html"
```

GPIO details can begin as provisional and calibration-friendly. The tool should preserve hand-edited
files and avoid destructive rewrites where possible.

The primary human workflow should not require remembering flags. Running
`lp-cli hardware manifest` should:

1. Load and validate/discover existing manifests.
2. Show a selectable list with display text like `seeed/xiao-esp32-c6 - Seeed XIAO ESP32-C6`.
3. Include actions for "add new manifest", "validate all", and "quit".
4. After choosing a manifest, offer show/edit/delete/validate/calibrate-next-step actions.

Explicit subcommands remain for tests, scripting, and recovery.

## Main Components And Interactions

1. `HardwareManifest` gains `target`, `vendor`, and `product` metadata. Existing Rust constructors
   can keep builder-style setters to avoid noisy tests.
2. `HardwareManifestFile` describes the TOML schema and converts to/from `HardwareManifest`.
3. `BoardManifestStore` finds `lp-core/lpc-shared/boards`, lists profile ids, loads TOML, writes new
   TOML, validates uniqueness, and rejects path traversal.
4. `lp-cli` adds a `Hardware` top-level command with an interactive `Manifest` default workflow and
   explicit `Manifest`/later `Calibrate` subcommands.
5. README/docs state that `lp-cli` is currently a developer-facing codebase tool.
6. Calibration later uses the manifest store as its persistence layer, marking observed labels and
   unsafe pins in the same board profile file.

## Validation Strategy

Prefer host-only tests first:

```bash
cargo test -p lpc-shared hardware
cargo test -p lp-cli hardware
cargo check -p lp-cli
```

Firmware calibration phases add RV32 checks only when firmware code changes.
