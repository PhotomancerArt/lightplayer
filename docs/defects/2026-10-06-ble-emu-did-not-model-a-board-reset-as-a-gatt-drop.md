---
status: fixed
found: 2026-10-06      # how: report (OTA M7 P12's scope, then `just walk-ota-ble-emu`)
fixed: this change
area: lpa-studio-web public/lpa-link/virtual_bluetooth.js (`?ble=emu`)
class: stand-in-divergence
related:
  - docs/defects/2026-10-02-the-ble-emu-polyfill-relays-lp-link-bytes-as-m-lines.md
  - docs/defects/2026-10-06-a-bluetooth-update-never-opens-the-reconnected-link.md
  - lp2025/2026-10-05-0820-ota-studio-ble-updates (P12)
---
# `?ble=emu` did not model a board reset as a GATT drop

**Symptom** — over `?ble=emu`, an over-the-air update's three board resets
reached Studio as lp-link session resets inside one live Bluetooth
connection ("board reset · reconnected in 0.3 s", `connects: 1` in the
polyfill's stats). A real board's radio goes down with its CPU, so on a
board every reset is a GATT drop and a reconnect. The walk passed on a path
no Bluetooth board takes, and Studio's real reconnect path across an
update's resets was never exercised.

**Root cause** — the polyfill stands in for the board's NUS service over the
emulated board's USB-Serial-JTAG link, and on a C6 that link survives a
chip reset (the ruling in `virtual_serial.js`). Nothing in the polyfill
watched for the reboot.

**Fix** — a SYN from the board after its link carried anything else is
either a link restart (radio up) or a reboot (radio down). The polyfill
holds the board's frames, reads the board's reboot count from the backing's
registry (`GET /boards`'s `reboots`, or the tab's row), and either drops
the GATT connection (`gattserverdisconnected`, counted as `resetDrops`) or
delivers what it held. A reset drop leaves the board's byte channel open:
the rebooted board already starts a new session. The guest cycle clock was
tried first and missed: a core-only boot that verifies its image can run
past the last control reply's cycle before its first SYN.

**Regression coverage** — `lpa-link/tests/browser_ble_conformance.rs`:
`a_board_reset_is_a_gatt_drop_and_the_session_reconnects_by_itself`,
`a_link_restart_without_a_reset_stays_one_connection`; `just
walk-ota-ble-emu` (every step checks `resetDrops`).

**Lesson** — a stand-in has to model the events the real thing has, not
just the bytes. Here the missing event was the radio going down with the
CPU, and its absence hid a real bug
(`2026-10-06-a-bluetooth-update-never-opens-the-reconnected-link`).
