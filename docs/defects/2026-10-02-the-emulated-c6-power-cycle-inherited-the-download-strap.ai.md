---
status: fixed
found: 2026-10-02      # how: e2e — the C6 repartition's migration walk (W7, a cable pull)
fixed: e9f26ea96
area: lp-emu/esp/lp-emu-esp32c6 machine.rs (ControlCommand::PowerCycle)
class: fidelity
related:
  - lp2025/2026-10-01-1843-c6-repartition
  - scripts/emu/walk-migration-emu.mjs
---
# The emulated C6's `power-cycle` came back in the DOWNLOAD strap after a download dance

**Symptom** — the migration walk pulls the cable from a USB-powered board
mid-update (`detach`, then the control channel's `power-cycle`). The
board came back `rst:0x1 (POWERON),boot:0x16 (DOWNLOAD(USB/UART0/SDIO_REI_FEO))`
and `waiting for download`; Studio's card read "Waiting in ROM download
mode — needs firmware" for a board that had a firmware image on it.

**Root cause** — the control channel's `PowerCycle` requested its reset
with `strap: self.strap`, and `self.strap` is the strap of the LAST reset.
The update's download dance resets the chip in the download strap, so the
power cycle after it inherited that. On silicon a power-on samples the
strapping pin (GPIO9, pulled up on a dev board → SPI boot); the USB
block's download request is cleared with the supply.

**Fix** — the machine keeps `pin_strap` (the builder's strap — the board's
pins) and a power cycle samples that; resets keep using the strap they
ask for.

**Regression coverage** —
`lp-emu-esp32c6 machine::tests::a_power_cycle_after_a_download_dance_boots_from_the_pins`
(fails against the old line, passes now).

**Lesson** — "the strap" was one field doing two jobs: what the pins say,
and what the last reset was asked for. A power cycle reads the first, a
`chip_rst` the second; once the model kept one, every power cycle after a
flash told a lie a walk could only see by pulling a cable.
