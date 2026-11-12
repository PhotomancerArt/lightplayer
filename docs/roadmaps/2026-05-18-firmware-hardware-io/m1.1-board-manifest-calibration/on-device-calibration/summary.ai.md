# On-Device Calibration Implementation Summary

## What Was Built

- Added `test_gpio_calibrate` firmware mode for ESP32-C6.
- Added a tiny serial protocol: `HELLO`, `PING`, `PULSE <gpio>`, and `STOP`.
- Firmware emits parseable `CAL ...` lines while opening, pulsing, stopping, or rejecting GPIOs.
- Added `lp-cli hardware calibrate` as the host-driven calibration loop.
- The CLI validates board target, opens serial, sends pulse commands, watches for firmware logs, and
  prompts with the happy-default `(q/p/y/N)` flow.
- Confirmed mappings write board-visible labels to manifest GPIO resources while preserving old
  labels as aliases.
- Confirmed crash/timeout pins are written as reserved resources.
- Resume state is saved under `target/hardware-calibration/`.
- Added a developer recipe: `just fwtest-gpio-calibrate-esp32c6`.
- Documented the calibration protocol and developer flow.

## Decisions For Future Reference

#### Host-Driven Calibration

- **Decision:** `lp-cli` owns session state, prompts, timeout handling, resume files, and manifest
  writeback.
- **Why:** The firmware stays small and the user can press Enter through the normal path while the
  host manages recovery.
- **Rejected alternatives:** Firmware-autonomous pin cycling.
- **Revisit when:** Calibration needs to run without a connected host.

#### One Label Per Run

- **Decision:** The first CLI loop calibrates the board-visible label currently connected to the
  scope and exits after a confirmed match.
- **Why:** It matches the physical workflow and avoids asking the user to move probes mid-loop.
- **Rejected alternatives:** Batch prompts for every silkscreen label in one run.
- **Revisit when:** The first board profile is calibrated and repetitive runs become annoying.

#### Structured TOML Rewrite

- **Decision:** Manifest updates use `HardwareManifestFile::write_toml()`.
- **Why:** The current manifest store already validates and rewrites structured TOML.
- **Rejected alternatives:** Comment-preserving surgical TOML edits.
- **Revisit when:** Hand-authored comments in board manifests become important.

## Validation

- `cargo fmt --check`
- `cargo test -p lp-cli hardware`
- `cargo test -p lpc-shared hardware`
- `cargo check -p lp-cli`
- `cargo run -p lp-cli -- hardware manifest validate`
- `cargo run -p lp-cli -- hardware calibrate --help`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,test_gpio_calibrate`
- `just --list`

Manual hardware validation with a connected ESP32-C6 and oscilloscope is still required.
