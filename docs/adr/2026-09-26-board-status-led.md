# ADR: A Board's Own LED Shows What LightPlayer Is Doing

- **Status:** Accepted
- **Date:** 2026-09-26
- **Deciders:** Photomancer (Yona asked for it while testing `PowerButton`
  deep sleep on the PLAYFUL choker)
- **Supersedes:** None
- **Superseded by:** None

## Context

`PowerButton` (PR #787, `docs/adr/2026-06-16-power-button-runtime-event.md`)
puts the ESP32-C6 into deep sleep. On a piece whose strip is dark, a board
that is asleep and a board that is awake with nothing lit look the same.
So does a board that has hung. Yona could not tell them apart at the desk.

The Seeed XIAO ESP32-C6 has an amber user LED on GPIO15, lit when the pin
is driven LOW. Seeed's pin map calls it "User Light". Zephyr's board file
declares it `GPIO_ACTIVE_LOW`, and RIOT's board.h says `LED0_ACTIVE (0)`.
Nothing in LightPlayer drove it.

## Decision

A board's own LED, where it has one, is LightPlayer's status light. What it
shows is a small fixed vocabulary, in `lpc_hardware::StatusLedState`:

| state         | pattern                               | when                                    |
|---------------|---------------------------------------|-----------------------------------------|
| `Booting`     | steady on                             | board known → server loop starts        |
| `Running`     | on, with a 100 ms dark blip every 2 s | the server loop is running              |
| `PoweringOff` | three flashes, then dark              | a power-off was accepted; deep sleep follows |
| (asleep)      | dark                                  | deep sleep: the pad is not held         |

- **`Running` blips rather than holding steady.** The blip needs the
  executor to keep scheduling. A board that is steady on without blipping
  has not finished booting, or has stopped.
- **`Booting` is steady because it has to be.** The boot runs to the server
  loop without yielding, so no task can animate it. The emulator run showed
  this: a 5 Hz boot blink never produced an edge.
- **Where the LED is lives in code, keyed on the board id in effect.** It is
  `lpc_hardware::board_status_led_for`, beside `board_quirks_for`, for the
  same reason: a `hardware.json` field would be a persisted-format change.
  The same pin is an ordinary GPIO on every other board.
- **The pin is reserved in the board manifest** (`reserved_reason`), so no
  project can claim it. A test holds the manifest and the table together.
- **No node and no project setting.** The light belongs to the firmware.

New states (a fault, a radio link, safe mode) are added to
`StatusLedState` as a pattern plus a test. Another board gets the light by
adding its pin to `board_status_led_for` and reserving it in its manifest.

## Consequences

- The XIAO C6 lights its LED from the moment the manifest loads, about
  where the RF-switch quirk is applied, until it sleeps. That is a few mA
  on a piece that already powers a strip.
- Before the manifest loads (ROM, bootloader, the first part of app init)
  the LED is dark. GPIO15 is a strapping pin (JTAG source select, which
  only matters with an eFuse the product never burns). The ROM has
  sampled it long before the firmware drives it.
- A firmware that is not `fw-esp32c6` (S3, classic, emu) drives no status
  LED yet. The table and the patterns are shared, so adding one is
  firmware wiring only.
