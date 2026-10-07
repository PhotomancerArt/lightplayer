---
status: fixed
found: 2026-10-06      # how: hardware walk (the M7 agent pre-walk on the fixture C6, Mac Chrome via CDP)
fixed: this change
area: fw-esp32c6 `split_boot` (core-only branch) × `Esp32EspNowRadioDriver` (owns the Wi-Fi controller) × BLE coexistence
class: lifecycle-ownership
related:
  - lp2025/2026-10-05-0820-ota-studio-ble-updates (P10, P13)
  - lp2025/2026-10-04-0757-ota-update-protocol/c-radio-links (Part C P1)
---
# Core-only drops the Wi-Fi controller, and Bluetooth goes off the air

**Symptom** — an X→Y update over Bluetooth (`lp-cli link capture
blepipe:`, Mac Chrome as the central) took the engine's offer, the board
reset into core-only, and the pipe page never found it again: 27 connects
in a row timed out at 12 s. A fresh Chrome scan saw the other desk board
and not this one, while the board's console said `[ble] advertising as
LP-b48c` and nothing else — no connection ever reached it. Over USB the
board was fine (core-only, "updating", waiting).

**Root cause** — `split_boot`'s core-only branch destructured the core's
boot with `..` and so dropped the ESP-NOW driver, which owns the
`WifiController`; dropping it deinitializes Wi-Fi. With the radios in
coexistence, Bluetooth stopped transmitting with it: the BLE host still
ran its advertising task (hence the log line) but nothing went on the air.
Before Part C nothing served Bluetooth in core-only, so no one looked.

**Fix** — core-only keeps the driver (`core::mem::forget`: core-only never
returns, every committed piece ends in a reset). Re-run on the fixture:
the engine-less board was found by the chooser at once, healed over
Bluetooth with no login, and every core-only leg of the following X↔Y
updates reconnected.

**Regression cover** — none on the host or the emulator: the emulator has
no BLE air (AGENTS.md, "Radio is the named exception"). The desk check in
`spikes/ble-lab/README.md` ("Updates over Bluetooth") is the cover; any
change to what core-only keeps from the boot needs that run.
