# Phase 4: Developer Wiring And Docs

## Scope Of Phase

Make the calibration workflow discoverable and convenient for developers running from the repo.

In scope:

- Add a justfile recipe for building/running the ESP32-C6 GPIO calibration firmware.
- Document how to flash/run calibration firmware and then run the host CLI loop.
- Document the firmware serial protocol.
- Update M1.1 notes or README references as needed.

Out of scope:

- Full `lp-cli` auto-build/auto-flash automation unless explicitly requested after the core loop is
  working.
- End-user packaging.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `justfile`
- `README.md`
- `docs/architecture.md`
- `docs/roadmaps/2026-05-18-firmware-hardware-io/m1.1-board-manifest-calibration/on-device-calibration/`
- optional new docs file near firmware or hardware manifest docs

Suggested just recipe:

```make
fwtest-gpio-calibrate-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --features test_gpio_calibrate,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}
```

Docs should include:

- Run firmware:

```bash
just fwtest-gpio-calibrate-esp32c6
```

- Run host calibration:

```bash
cargo run -p lp-cli -- hardware calibrate esp32c6 --board seeed/xiao-esp32-c6 --port serial:auto
```

- Explain happy-path keys:
  - Enter / `N`: no, advance
  - `y`: yes, record mapping
  - `p`: previous pin
  - `q`: quit

- Explain that timeout/crash detection is conservative and asks before marking a pin dangerous.
- Mention that the first writeback may rewrite TOML formatting.

## Validate

```bash
cargo fmt --check
cargo check -p lp-cli
just --list
```
