---
status: open
found: 2026-10-05      # how: hardware-walk — PR #971's P10 re-run, bench C6 A0:F2:62:87:B4:8C, image b56bcd144
area: fw-esp32c6 reboot path (`[REBOOT] client requested a restart`) × the IDF second-stage bootloader (espflash 3.3.0's bundled)
class: assumed-context
related:
  - docs/defects/2026-10-05-the-host-flasher-dropped-the-split-images-last-bytes.md
  - docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md
  - lp2025/2026-10-04-0005-ota-split-image-ships (data/p10-tailfix-a0f26287b48c/link-reboot-attempt.txt)
  - lp2025/2026-10-04-0005-ota-split-image-ships (data/reset-crash-ab/summary.txt)
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

**Root cause** — established 2026-10-05 by a silicon A/B
(`lp2025/2026-10-04-0005-ota-split-image-ships/data/reset-crash-ab/summary.txt`).
**Not specific to the split image, and pre-existing on main.** Main's
monolithic image (`d52f32df1`) crashed the bootloader on 11 of 22 warm resets
(RTS `rst:0x15` and requested reboot `rst:0x3`) with Bluetooth on and the
project running, and on 1 of 8 with projects stopped. The split image (#971,
`9177adfa3`/`b56bcd144`) crashed on 1 of 40 here (3 of 45 counting the
earlier P10 runs). The board's old firmware (`30ed7c05edb0`) crashed on 0 of
14. **Bluetooth off removes it**: 0 of 10 on main and 0 of 10 on split,
toggled with `accessSetSwitches.bleEnabled` and a reboot.

Every crash carries the identical registers from the original report:
`PC 0x4087073a`, `RA 0x408707d0`, `SP 0x4087e1d0`, `MCAUSE 0x2`,
`MTVAL 0x10`. In espflash 3.3.0's bundled bootloader, `0x4087073a` is
`sw zero,0x1c(sp)`, reached right after the ROM `memcmp` that checks the
app image's SHA-256 against the computed one — the hash matched, so the
code branched into the corrupted spot.

Memory dumped while the bootloader hung (seven dumps, identical across main
and the split image) found exactly two words, at `0x40870734` and
`0x40870738`, reading `0x00100000` — shaped like a DMA descriptor's first
word, not proof on its own. In the running app, `HEAP_DRAM2` covers
`0x4086e610..0x4087e610`, which is where the bootloader's own IRAM lands
after an HP-only reset. So **the Bluetooth controller, which keeps running
across an HP-only reset, writes into memory the bootloader now occupies.**

Main crashes more often than the split image purely because of timing: main
hashes a ~3 MB app and reaches the clobbered instruction at ~730 ms after
the ROM starts the bootloader, where the split image's ~2.8 KB loader
reaches it at ~125 ms — a ~5.5x wider window for the stray write to land,
against an observed crash-rate ratio of ~7x.

**Consequence**: the app's RTC watchdog pulls the board back after ~9 s, and
the recovery ledger then wrongly blames the user's shader node ("hang
detected by hardware watchdog"), which can escalate against a healthy
project.

**Fix** — none yet. Untested directions, either of which should cover it:
- keep the Bluetooth controller's DMA buffers out of the address range the
  bootloader loads into (exclude it from the heap the controller allocates
  from);
- and/or stop or reset the controller before a software reset. An RTS reset
  from the host can't be intercepted, so the placement fix is the one that
  covers both reset paths.

This matters for OTA (M4): an update ends in exactly this reboot.

**Regression coverage** — none. The emulator models no radio DMA.

**Lesson** — a software reset is not a power cycle. Anything outside the HP
domain keeps running into memory the next stage now owns.
