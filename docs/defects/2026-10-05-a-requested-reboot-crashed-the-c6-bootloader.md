---
status: open
found: 2026-10-05      # how: hardware-walk — PR #971's P10 re-run, bench C6 A0:F2:62:87:B4:8C, image b56bcd144
area: fw-esp32c6 reboot path (`[REBOOT] client requested a restart`) × the IDF second-stage bootloader (espflash 3.3.0's bundled)
class: assumed-context
related:
  - docs/defects/2026-10-05-the-host-flasher-dropped-the-split-images-last-bytes.md
  - docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md
  - lp2025/2026-10-04-0005-ota-split-image-ships (data/p10-tailfix-a0f26287b48c/link-reboot-attempt.txt)
---
# A requested reboot crashed the C6's second-stage bootloader with an illegal instruction

**Symptom** — seen once, on silicon. The bench C6 ran the fixed split image
(`b56bcd144`) with its project at 37–39 fps and Bluetooth advertising.
`lp-cli link capture … --request reboot` was answered (`[REBOOT] client
requested a restart`). The chip reset (`rst:0x3 (LP_SW_HPSYS)`). The ROM
loaded the second-stage bootloader, which printed its partition table and
the loader's four segments. Then, before `Loaded app from partition`:

```text
Guru Meditation Error: Core 0 panic'ed (Illegal instruction)
PC      : 0x4087073a  RA      : 0x408707d0  SP      : 0x4087e1d0
MCAUSE  : 0x00000002  MTVAL   : 0x00000010
```

The USB device then dropped (`Broken pipe` on the host). The board came back
by itself: Studio, in the user's browser, connected to it within seconds.

Two `LP_SW_HPSYS` resets earlier the same day (the engine's panics, P10's
first run) went through the same bootloader cleanly.

**Root cause** — not known. One fact narrows it: `0x4087073a` is in the
bootloader's second segment (`0x4086e610..0x40871378`), and the bundled
`esp32c6-bootloader.bin` holds `0xce02` there, not `0x0010`. The
bootloader's code in IRAM was overwritten after the ROM loaded it. Something
that survives an `LP_SW_HPSYS` reset wrote into HP SRAM while the bootloader
ran.

Hypothesis, untested: the Bluetooth controller (modem domain) keeps
receiving into buffers that were the app's heap and are now the
bootloader's IRAM. ESP-IDF's `esp_restart` stops the radios and resets the
modem before its software reset, and our reboot may not. The panic resets
that worked happened during project load, which may have been before
advertising began.

**Fix** — none yet. Next steps:
- Repeat `--request reboot` on silicon with Bluetooth off and on.
- Read what the reboot path does before `software_reset`.
- Compare the monolithic image, to tell whether the split image matters.
  Nothing in the split image is known to.

This matters for OTA (M4): an update ends in exactly this reboot.

**Regression coverage** — none. The emulator models no radio DMA.

**Lesson** — a software reset is not a power cycle. Anything outside the HP
domain keeps running into memory the next stage now owns.
