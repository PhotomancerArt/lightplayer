# Fixtures

## `choker-lens-sample.txt`

114 `M!` lines (231,534 bytes) of real wire traffic, both directions, one per
line as `<dir> M!{json}` (`<` board→host, `>` host→board), in stream order.

- **Source:** `choker-lens-2026-09-23.tap`, a wire-tap recording from the BLE
  remote-control planning work
  (`~/.photomancer/planning/lp2025/2026-09-23-1428-ble-remote-control/wire-tap/`),
  captured 2026-09-23 by the opt-in tap in `emu serve`'s byte pump
  (`LP_EMU_WIRE_TAP`): the `fw-esp32c6` image on the emulated ESP32-C6, running
  the public PLAYFUL choker catalog project with a Studio device lens open for
  467 s. It is "Run D" in that plan's `spike-results.md`, and the corpus of the
  ion-wire spike's measurements.
- **Cut with:** `sample_tap.py` in this directory
  (`python3 sample_tap.py <recording.tap> choker-lens-sample.txt`). It
  reassembles each direction into lines and samples `M!` lines evenly per
  message class: 18 lens replies (`projectRead.events`), 24 lens requests
  (`projectRead.handle`), 24 heartbeats, 8 knob turns (`panel_write`) and 6 of
  their responses, one upload chunk (`filesystem.writeChunk`, host→board), and
  up to 3 of every other class (hello, file writes, loads, listings).
- **Contents:** no secrets. Board identity in it is the emulated board's MAC
  and build stamp.

Do not hand-edit it: re-cut it from a recording, and update the counts above.
