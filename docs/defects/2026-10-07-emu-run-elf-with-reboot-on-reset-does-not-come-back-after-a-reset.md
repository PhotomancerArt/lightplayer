---
status: open
found: 2026-10-07      # how: live-debugging (the Wi-Fi relay cell's first design, PR #1019, P9)
area: lp-cli `emu run --elf … --reboot-on-reset` × lp-emu-esp32c6 `restart()` (a direct-load run after a reset)
class: unexplained-stall
related:
  - docs/defects/2026-10-07-the-emulated-c6-cleared-rtc-fast-memory-on-every-reset.md
  - docs/defects/2026-09-29-the-emulated-c6-does-not-perform-a-software-reset.md
  - docs/reports/2026-10-07-wifi-relay-emulator-walk.md
---
# An `emu run --elf --reboot-on-reset` board does not come back after a reset

**Symptom** — the relay cell's first design restarted the board's side of a
run by resetting an emulated C6 started with `lp-cli emu run --elf <fw-esp32c6>
--reboot-on-reset` (and `--lan` with an uplink). After the reset the run
printed `machine: … rebooting into strap app` and then nothing: **no console
output for 400 s**, the board never answered its link again. The cell was
rewritten to boot twice over one `--flash` file instead (as a fielded board
is rebooted), and that passes every time.

**Root cause** — not diagnosed. Known: the same flag with the shipped image
restarted a board on 2026-09-29 (`link capture --request reboot` against
`emu run --reboot-on-reset`; that entry does not say whether it was a
direct load), so this is either specific to the direct-load
(`--elf`) path with a LAN and an uplink attached, or a regression since.
Untested, so not a hypothesis worth more than a place to start: the same run
without `--lan`; the same run with `--merged` (ROM-up) instead of `--elf`;
whether the strap-app reboot of a direct-load run finds an image to boot. The
RTC-memory fix of the same day (`restart()` now keeps LP SRAM on an HP reset)
landed the same day and may have moved it either way; the order was not recorded.

**Fix** — none yet. The cell does not depend on it.

**Regression coverage** — none: no minimal reproduction exists. A fix should
land one: an `emu run --elf --reboot-on-reset` board that is reset (a request
`reboot`, or the watchdog) and must answer again, on a direct load with a
network seam engaged.

**Lesson** — a restart that works in one boot path (ROM-up from a flash
image, over a `--flash` file) and not in another (direct load) is two
implementations of "reset" that the emulator's tests have only ever pinned on
one of them; a test that restarts a board should name which boot path it is
standing on.
