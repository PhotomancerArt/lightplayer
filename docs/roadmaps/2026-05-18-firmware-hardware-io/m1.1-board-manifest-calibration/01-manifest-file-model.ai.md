# Phase 1: Manifest File Model

## Scope Of Phase

Add the board manifest file schema plus target/vendor/product metadata to shared hardware types.
Seed the first Seeed XIAO ESP32-C6 manifest as a checked-in TOML file.

Out of scope: `lp-cli` commands, firmware calibration mode, and making firmware load TOML files.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep tests at the bottom of Rust files.
- Keep runtime identity address-based; labels, target, vendor, and product are metadata.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update `lp-core/lpc-shared/src/hardware/hardware_manifest.rs`:

- Add optional or required `target`, `vendor`, and `product` fields. Builder-style setters are fine
  if they keep test constructors readable.
- Add a small `HardwareTarget` or similarly named enum with at least `Esp32c6` and `Rv32imacEmu`.
- Add accessors.
- Keep `board_id`/`board_name` or migrate carefully only if it is low churn. Do not break existing
  M1 behavior.

Add `lp-core/lpc-shared/src/hardware/hardware_manifest_file.rs`:

- Serde-friendly TOML shape with fields:
  - `id`
  - `target`
  - `vendor`
  - `product`
  - `description`
  - `url`
  - GPIO/resource entries with internal address, display label, aliases, location, capabilities,
    and reserved reason.
- Conversion to `HardwareManifest`.
- Validation for non-empty id/target/vendor/product, known target value, URL presence when supplied,
  unique resource addresses, and valid hardware addresses.

Add `lp-core/lpc-shared/boards/seeed/xiao-esp32-c6.toml`:

```toml
id = "seeed/xiao-esp32-c6"
target = "esp32c6"
vendor = "seeed"
product = "XIAO ESP32-C6"
description = "Seeed Studio XIAO ESP32-C6 board profile."
url = "https://www.seeedstudio.com/Seeed-Studio-XIAO-ESP32C6-p-5884.html"
```

GPIO entries can start minimal/provisional if exact labels are not known yet, but include enough
structure for the CLI to show and validate the manifest.

## Validate

```bash
cargo test -p lpc-shared hardware
cargo check -p lpc-shared --no-default-features
```
