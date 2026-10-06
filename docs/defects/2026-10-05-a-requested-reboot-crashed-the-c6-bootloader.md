---
status: fixed
found: 2026-10-05      # how: hardware-walk — PR #971's P10 re-run, bench C6 A0:F2:62:87:B4:8C, image b56bcd144
fixed: cfe5598bd       # PR #985
area: fw-esp32c6 heap placement (`c_heap`, `board::esp32c6::init`) and reset path × the IDF second-stage bootloader (espflash 3.3.0's bundled)
class: assumed-context
related:
  - docs/defects/2026-10-05-the-host-flasher-dropped-the-split-images-last-bytes.md
  - docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md
  - docs/defects/2026-09-24-ble-enabled-c6-refuses-a-project-switch-after-the-heap-cut.md
  - docs/adr/2026-09-02-esp32c6-ram-split.md (Amendment 2026-10-05)
  - lp2025/2026-10-04-0005-ota-split-image-ships (data/p10-tailfix-a0f26287b48c/link-reboot-attempt.txt)
  - lp2025/2026-10-04-0005-ota-split-image-ships (data/reset-crash-ab/summary.txt)
  - lp2025/2026-10-05-1700-ble-warm-reset-fix (data/summary.txt)
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

**Why the controller's memory was there.** It was put there on purpose.
On 2026-09-24, `lp-fw/fw-esp32c6/src/c_heap.rs` started placing the radio
blobs' C heap (`malloc` and friends, which hold the controller's
descriptors and packet buffers) in the reclaimed `dram2_seg` region first.
The goal was to keep those blocks from splitting the main region
(`docs/defects/2026-09-24-ble-enabled-c6-refuses-a-project-switch-after-the-heap-cut.md`).
`dram2_seg` was believed free "once the app runs", and that is true only
until the next reset. Both bootloaders we ship load into
0x4086B910..0x40876F20 and run their stack at the top of `dram2_seg`. The
old image's 0/14 does not contradict this: it had the same placement, and
an older allocation order most likely put the descriptor on bootloader
bytes that are dead by the time the hash check runs.

This matters for OTA (M4): an update ends in exactly this reboot.

**Fix** — PR #985, in two halves:
- **Placement, the fix.** The radio's C heap gets its own 48 KiB region,
  `HEAP_RADIO`, in main RAM's `.bss` (0x4081A8E0..0x408268E0 in the shipped
  image), carved from the main heap region. When it is full, a block falls
  back to the main region, never to `dram2_seg`. `dram2_seg` carries no
  capability tag, so only Rust's global allocator, which no radio DMA
  reads, can land there. Heap totals are unchanged; the cost is contiguity
  (largest free block 181,600 → 132,458 B on `lp-emu:esp32c6:t1`; see the
  RAM-split ADR's 2026-10-05 amendment).
- **Quiesce, the second half.** Every firmware-initiated reset
  (`board::esp32c6::restart`) holds the modem's blocks in reset
  (`MODEM_SYSCON.MODEM_RST_CONF`) before `software_reset`. A host RTS
  reset never runs that code, which is why placement is the fix.

**Verified on silicon**, the same bench C6, 2026-10-06 01:18–01:51 UTC
(`lp2025/2026-10-05-1700-ble-warm-reset-fix/data/summary.txt`). Bluetooth
was advertising and the project running for ~15 s before each reset.
Resets alternated RTS and requested reboot.

| image | RTS | reboot | total |
|---|---|---|---|
| the fix, monolithic (`4d8b3a500`, main's long hash window) | 0/11 | 0/11 | **0/22** |
| the fix, shipped split image (`4d8b3a500`) | 0/11 | 0/11 | **0/22** |
| unfixed main `d52f32df1`, same sitting | 2/6 | 3/6 | 5/12, identical registers |

The RTS rows test placement alone, because the quiesce never runs on an
RTS reset. On silicon the radio region's high-water was 35,648 B of
49,152 B, with nothing overflowing to the main region.

**The recovery ledger's misattribution is not fixed here.** A follow-up
should make the ledger tell "the app hung in node X" from "the boot never
reached the app". The reset reason cannot do it: whichever watchdog pulls
the board back, it is an RTC-WDT reset. That is either the bootloader's own
9 s RWDT or the previous app's still-armed 8 s one, since an HP-only reset
does not stop the RWDT. That reason is the same one an app hang produces.
With the bootloader crash gone, the trigger is gone too.

**Regression coverage** — no test reproduces the DMA: the emulator models
none. What the emulator does check is the layout the fix depends on.
`just heap-budget-check-chips-c6` pins the largest free block that the new
region produces. The shipped image boots with the new layout through every
`test-emu-c6` gate. Each boot logs a `[radio-heap]` line, so the region's
size stays a measured number.

**Lesson** — a software reset is not a power cycle. Anything outside the HP
domain keeps running into memory the next stage now owns. Memory the app
"reclaims" from an earlier boot stage is only free until the next reset.
Nothing that outlives that reset may point into it.
