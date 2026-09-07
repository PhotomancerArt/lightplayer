---
status: fixed
found: 2026-09-06      # how: report (Yona, first LightPlayer install on a fresh Seeed XIAO ESP32C6, prod Studio)
fixed: this change
area: lpa-link flashers (browser esptool-js + host espflash) · lpa-devices post-flash reconnect ladder · the merged image's second-stage bootloader
class: assumed-context
related:
  - 2026-09-06-xiao-c6-7e44-hangs-in-the-second-stage-bootloader.md
  - ~/.photomancer/planning/lp2025/2026-09-06-1649-c6-first-flash-bootloader-hang/
  - lp-app/lpa-link/src/providers/host_serial_esp32/lp_analog_i2c.rs
  - scripts/c6-bootloader-hang-walk.sh
---
# A fresh ESP32-C6's first flash succeeds, then the board hangs in our bootloader until someone replugs it

**Symptom** — Studio (prod) wrote the C6 image onto a factory-fresh Seeed
XIAO ESP32C6, then:

```text
Wrote 2478128 bytes (1430198 compressed) at 0x0 in 8.292 seconds.
Hard resetting via RTS pin...
rst:0x15 (USB_UART_HPSYS),boot:0x1f (SPI_FAST_FLASH_BOOT)
Saved PC:0x40800832
…
rst:0x7 (TG0_WDT_HPSYS),boot:0x1f (SPI_FAST_FLASH_BOOT)
Saved PC:0x4086ed7a          ← ×13, identical
…
rst:0x15 (USB_UART_HPSYS),boot:0x17 (DOWNLOAD(USB/UART0/SDIO_REI_REO))
waiting for download
firmware was written, but the board never answered. If it is on a CH340/V3 bridge, unplug it, …
```

and the card ended on "Waiting in ROM download mode — needs firmware" after a
flash that had succeeded. Historically "unplug it and plug it back in" made
it go away, which is why nobody had looked.

**Root cause** — three things stacked:

1. The board's previous firmware (Seeed's factory ESP-IDF app) gates the LP
   peripheral clocks it does not use at startup — `LPPERI_CLK_EN`
   (`0x600b2800`) bit 29, `LP_ANA_I2C_CK_EN`, among them. Read from the
   stuck board: `0x41000000` against a power-on `0x7f000000`. ESP-IDF apps
   never miss it: they drive the analog bus through the HP aperture
   (`I2C_ANA_MST`, `0x600af800`).
2. The second-stage bootloader in our merged image (espflash 3.3.0's bundled
   ESP-IDF `v5.1-beta1-378`) drives it through the **LP** aperture
   (`LP_I2C_ANA_MST`, `0x600b2400`). Its first regi2c write in `rtc_clk_init`
   (slave `0x6d`, register `0x0e`) latches the master busy with no clock to
   finish it, and the bootloader spins at `0x4086ed7a` — inside its own
   second load segment — until the ROM-armed TG0 flash-boot watchdog resets
   it. Sometimes; the induced repro hangs silently.
3. Every reset a flasher can send — USB-Serial-JTAG's RTS reset, the
   watchdog — resets the HP domain only. The LP domain, that clock gate
   included, survives. Only a power-on reset restores it. The flasher, the
   bootloader and the ladder all *assumed* a reset yields a board in its
   power-on context; it does not.

The ladder made it worse: rung 3 (`BothThenDrop`, the CH34x whole-status
sequence) selects the ROM downloader on a USB-Serial-JTAG chip, which is
where the "needs firmware" status came from. And the exhausted-ladder copy
was written for CH340 bridges.

LightPlayer's own HAL (esp-hal 1.1.1, esp-radio 0.18) never touches
`LPPERI`, so a board that already ran LightPlayer never hits this — it is a
first-flash-on-a-fresh-board defect. The same `Saved PC` was filed the same
morning against a second XIAO on the UART-bridge bench (the related entry)
and blamed on the fixture wiring.

**Fix** — three places:

- Both flashers, on C6 only and only when the clock reads gated or the
  master reads busy, set bit 29 **and pulse `LPPERI_RESET_EN`
  (`0x600b2804`) bit 29** over the stub before their closing hard reset, and
  log one line: `restored the LP analog I2C clock the previous firmware left
  gated (LPPERI_CLK_EN 0x41000000 -> 0x61000000, busy cleared)`. Setting the
  clock alone does not clear the latched busy (bench). Host side:
  `lp_analog_i2c.rs` + `restore_lp_analog_i2c_clock` in
  `host_esp32_flash.rs` (the flash path now connects with `NoResetNoStub`
  and resets by hand afterwards); browser side: `restoreLpAnalogI2cClock` in
  `browser_esp32_flash.js`.
- The ladder never sends `BothThenDrop` to a native-USB board, and a ROM
  boot line whose `Saved PC` lies inside the bootloader's code segments
  (`lpa_devices::bootloader::bootloader_code_ranges`) ends the ladder at once
  with: the bootloader hung after the reset — unplug, replug, Reconnect.
- `lp-cli firmware package` checks the merged image's bootloader segments
  against that table, so a bootloader swap cannot silently blind the
  detection.

The bench proof, both directions, on XIAO `A0:F2:62:85:A8:7C` (2026-09-06):
clearing the bit over the ROM reproduces the hang on demand; the sequence
above followed by Studio's ordinary reset boots LightPlayer.

**Regression coverage** — `lpa-link`
`lp_analog_i2c::tests` (the register plan and the log line); `lpa-devices`
scenarios `a_native_usb_board_after_a_flash_never_gets_the_ch34x_rung`,
`a_hung_bootloader_saved_pc_ends_the_ladder_early_with_replug_guidance`,
`a_saved_pc_outside_the_bootloader_is_not_a_hang`; the studio-core e2e
`post_flash_silence_climbs_the_ladder_then_fails_with_honest_guidance`
(native-USB ladder). The flash itself needs a board:
`scripts/c6-bootloader-hang-walk.sh <MAC>` induces the fault, flashes
through the host provider and asserts the hello; the browser path is the
same walk by hand through Studio. The JS layer has no harness
(`docs/debt/web-serial-js-untestable.md`).

**Lesson** — a reset is not a power cycle. On the C6 family the LP domain
carries clock gates, PMU and PLL state across every reset a host can send,
and *whatever ran before us* decides what our bootloader wakes up to. The
ritual users learned ("unplug it and plug it back in") was a power-on reset
in disguise; encoding it means restoring the specific state, not asking for
the ritual. Two habits follow: read the ROM's `Saved PC` against the
bootloader's `load:` segments before blaming firmware (the address
symbolizes to an app `HEAP` symbol by coincidence), and when a board only
misbehaves after a *specific* previous firmware, dump the LP-domain
registers over the ROM downloader before power-cycling — that diff is the
whole diagnosis.
