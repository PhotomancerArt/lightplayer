# ADR: The C6 boots through LightPlayer's own second-stage bootloader

- **Status:** Accepted (G0, Yona, 2026-10-17); held with the morning cutover
- **Date:** 2026-10-17
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Related:** [2026-10-04-c6-split-link-firmware-loader-and-boot-records](2026-10-04-c6-split-link-firmware-loader-and-boot-records.md)
  (the loader, the records, `choose`; amended below),
  [2026-07-28-esp32c6-flash-budget](2026-07-28-esp32c6-flash-budget.md),
  [2026-09-10-the-emulator-first-device-walk](2026-09-10-the-emulator-first-device-walk.md).
  Planning: `lp2025/2026-10-17-0130-c6-own-bootloader` (`plan.md`, `notes.md`).

## Context

Until now a C6 booted **mask ROM → ESP-IDF second stage (at `0x0`) → our
RAM-only loader (at `factory+0`) → core → engine**. The IDF stage was
espflash 3.3.0's bundled build of ESP-IDF v5.1-beta1, configured by
espflash's maintainers. It cost:

- ~135 ms of every boot (power-on to app entry: 166 ms on CX1), most of it
  printing 22 lines at 115,200 baud;
- the two-mapped-flash-segment assert (`rom_index < 2`) that shaped our link
  for a year (`.eh_frame`, `.rodata_desc`, `build.rs` linker-script patches,
  the loader's two placeholder segments);
- the LP analog master wedge: a board whose previous firmware gated the LP
  analog I2C clock hangs in the IDF's clock bring-up for ever
  (`docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md`),
  worked around in both flashers and in Studio's `Saved PC` table;
- a page size (32 KiB), a watchdog arming and a brownout configuration
  chosen inside a binary we did not build;
- RAM the app reclaims (`dram2_seg`) holding live boot code while a radio's
  DMA could still be running (`2026-10-05-a-requested-reboot…`).

We already own every other byte of the boot: the loader, the records, the
core. The emulator runs the real mask ROM and the real IDF bootloader, so the
IDF's effect on the chip can be **measured** rather than read from source.

## Decision

**1. Our loader is promoted to the second stage (option (a)).**
`lp-fw/fw-esp32c6-loader` builds two flavours from one crate:

- the **factory** flavour (default; `loader.x`), unchanged byte for byte,
  at `factory+0`, which the IDF bootloader starts as its app — the **IDF
  door**;
- the **bootloader** flavour (`--features bootloader`; `bootloader.x`),
  which the mask ROM loads from flash `0x0` — **our door**. It runs a short
  prologue (§3), prints one line, `[LPBOOT] LightPlayer bootloader v1`, and
  then does the loader's job with the same code: read the records, `choose`,
  map the core, copy its RAM segments, fall back, jump.

Rejected: (b) a Rust stage that chains the factory loader (an extra hop and
a second ESP-image reader at boot, nothing gained); (c) a quiet rebuild of
the IDF bootloader as the shipping answer (PR #1168 — kept only as a
fallback: it keeps every IDF failure class but the logging, and a quiet
bootloader that hangs prints nothing).

**2. `app.bin` is byte-identical under both doors.** A board moves between
them by rewriting `0x0..0x8000` only. `lp-fw-split` writes both merged
images — `merged-lp.bin` (ours) and `merged-idf.bin` (the IDF's) — and
`merged.bin` is the build's door, ours by default (`--door idf` for the
other). Boards updated only over the air keep the IDF door indefinitely:
OTA never writes `0x0`. Every Studio USB install writes ours, no opt-in
(G0 Q5); a board is put back on the IDF door by a USB flash of a
`merged-idf.bin` (or a release from before the cutover).

**3. Minimum faithful (D3–D6).** The prologue reproduces only the state the
IDF leaves that a core inherits, each value **measured** (§"The core entry
contract"), and does nothing else: no clocks, no PLL, no regi2c, no `LPPERI`,
no RNG entropy, no partition table, no image verification, no flash-mode
change, no SPI1 command, no USB-Serial-JTAG or GPIO 12/13. The CPU stays on
the crystal until `esp_hal::init`.

**4. We write the image ourselves (D7).** `tools/lp-fw-split`'s
`bootloader_image` writes the header, the segments, the XOR checksum and the
appended SHA-256 from the bootloader-flavour ELF. No espflash image builder,
no bundled binary at `0x0`. The IDF binaries stay as the IDF door's images
and as test fixtures.

**5. Identity (D8).** Our image carries `LPLB` + version (1) at image offset
`0x20` (`lp_bootctl::bootloader_identity`); `LPLV` at `factory+0` is
unchanged. No wire change: the door is the boot log line (G0 Q6).

**6. Both doors are gated on every split image, forever.** The emulator's
split-boot gate boots every split image ROM-up through both; the lp-cli split
and OTA scenarios run with `LP_BOOT_DOOR=idf` as well as by default.

## What the mask ROM requires (measured on the emulator, real ROM, 2026-10-17)

Variants of our image at `0x0`, booted ROM-up (`lp-emu:esp32c6`, the
vendored `esp32c6_rev0_rom.elf`; the ROM's accept/refuse logic is silicon's
code, its timing is not):

| variant | ROM |
|---|---|
| header chip id `0x0005` (C3) | **refused**: `Invalid chip id. Expected 13 read 5`, then `ets_main.c 331` (no boot) |
| min full revision 1.0 / max full revision 0.0 | **ignored** (boots) |
| checksum byte wrong | **refused**: `Checksum failure. Calculated 0x87 stored 0x86` |
| appended SHA-256 wrong, or `hash_appended = 0` | **reported, not refused** with secure boot off: `SHA-256 comparison failed: …`, then `entry` and boot. The ROM checks a hash whatever byte 23 says; we always append the right one |
| a segment in the flash window (`0x4200_0000`) | **fault**: the ROM stores to it (`Store/AMO access fault`) |
| a segment in the ROM's own stack (`0x4087_D000`) | loaded (emulated; never do this on silicon) |
| 16 segments | boots |
| 17 segments | **refused** (`ets_main.c 331`) |
| header mode QIO | printed `mode:QIO` and booted in the emulator, which does not model the bus — silicon would need the QE bit first. We keep DIO |

So: chip id `0x000D`, ≤ 16 RAM-only segments below `0x4086_AD08`, a correct
checksum, a correct appended SHA-256 (hygiene, not enforcement), end before
the partition table at `0x8000`. Ours is one segment, `0x4085_0000`,
~3.5 KB.

## The core entry contract (C1–C8)

What a core — any core, installed over the air into either door — may assume
at its first instruction. Measured as the register state at the core's first
fetch on the emulator through each door (every MMIO write since power-on,
last value per register), then diffed. "Ours" lists the prologue module that
does it.

| ID | Item | IDF door (measured) | Ours |
|---|---|---|---|
| C1 | MMU | `mmu_power_ctrl[4:3] = 1` (32 KiB; ROM leaves 64 KiB), every one of 256 entries invalidated (content 0), then the loader's maps; entry 0 = the core's first page; cache enabled (`l1_cache_ctrl = 0`, as the ROM hands over) | `cache_mmu`: suspend, page mode, unmap all, invalidate, resume; then the same loader code |
| C2 | RAM | core RAM segments in place; nothing of the boot stages needed after the jump | same; the stage lives in `0x4085_0000..0x4086_0000` (inside the core's `.bss`), never in `dram2_seg` |
| C3 | Flash | `PCR.mspi_clk_conf` HS divider 5; SPI0/SPI1 `clock = 0x8000_0000`; SPI0 `ctrl2[9:0] = 0` (ROM leaves `0x41`); flash pads GPIO 24–30 (not 27) drive 1 (ROM: 2); SPI1 left after its last IDF command (`user 0x800000C0`, `user1 0x5C000007`, `user2 0x70000004`, lengths/addr/w0 0); **the ROM's flash descriptor at 4 MB** | `flash_setup` (no SPI1 command issued), `rom_flash_chip` |
| C4 | Watchdogs | TIMG0 MWDT flash-boot protection off (bit 14 cleared, bit 22 update); RWDT `wdtconfig0 0xC007_EA00` (on, stage 0 = system reset), `wdtconfig1 0x0009_56A0` (~9 s), fed; SWD auto-feed (bit 18) | `watchdogs` |
| C5 | LP domain | untouched but for: LP interrupts masked and cleared (`LP_WDT`, `LP_TIMER`, `LP_ANA`, `PMU` `int_ena = 0`), `LP_AON.store4 = 0x0028_0028` (the 40 MHz crystal word ROM routines read) | `lp_interrupts`, `debug_assist` |
| C6 | Brownout / analog reset | `LP_ANA.fib_enable` bits 2 then 1 cleared; `bod_mode1_cntl` bit 31 set | `analog_reset` (unchanged, G1 Q2 open) |
| C7 | Clocks | the IDF switches to the PLL (`sysclk_conf`, `cpu_freq_conf`), sets modem and LP clock gating, `fosc`/`rc32k`, `ctrl_tick_conf` — **all redone by `esp_hal::init`** (`rtc_cntl::rtc::esp32c6::init`, `soc::esp32c6::clocks`) | not done: the core starts on the crystal |
| C8 | Identity | `LPLV` v1 at `factory+0` | the same, plus `LPLB` v1 at `0x0` |

Also left by the IDF and reproduced: the access-permission managers' filters
off (`HP_APM`/`LP_APM0`/`LP_APM` `func_ctrl = 0`) and the PC recorder on
(`ASSIST_DEBUG.cpu0.rcd_en = 3`, so the ROM can print `Saved PC:` after a
reset).

**Dropped** (the G1 drop list): the analog bus (`LP_I2C_ANA_MST`,
`I2C_ANA_MST`, `MODEM_LPCON`/`MODEM_SYSCON`), the PLL and clock tree, the RNG
early-entropy source (SAR ADC, `PCR.saradc_*`, `PMU.rf_pwc`), the IDF's own
console, the app image's verification and SHA, the partition table, eFuse
checks, and every SPI1 command (the IDF's status reads and `RDID`). After
those, the two doors' states at core entry differ only in registers
`esp_hal::init` rewrites and in `SPI1.cmd` (a self-clearing trigger).

### What the emulator caught

The first bring-up through our door booted on the emulator, and the core
then told the board its files were "in the tree store, a newer filesystem"
and locked them. The ROM's flash routines (which esp-storage reads through)
refuse any address past their descriptor's `chip_size`; the ROM's default is
smaller than 4 MB and the IDF corrects it from the header
(`esp_rom_spiflash_config_param`, which only stores six words). The
partition table and the loader (below 2 MB) read fine; every read of `lpfs`
at `0x35_0000` failed, and an unreadable partition counts as a tree store.
The fix is C3's last item. On silicon this would have locked every
board's files on its first boot through our door.

## Silicon (CX1, 2026-10-17)

CX1 (`c6-expendable`, XIAO C6, MAC `14:C1:9F:E6:54:90`), one sitting, the
same build through both doors (`94667b023`; records in the planning dir's
`results/cx1/`). `configuration=silicon:esp32c6`.

| Check | Result |
|---|---|
| ROM accepts our image; one line | `load:0x40850000,len:0xdc0` → `entry 0x40850008` → `[LPBOOT] LightPlayer bootloader v1` → `[LOADER] core @0x18000 (proven)`; no checksum or SHA complaint |
| `boot` (power-on → app entry, the `[perf] first light` line), 10 power-on boots per case | **27 ms** every boot (ours) vs **167 ms** every boot (IDF door, same build, same board) |
| First light, `projects/test/basic` | **475 ms** (ours) vs 613 ms (IDF) |
| esp-storage after our stage | `RDID` sized 4 MB; `lpfs` mounts; boot-record marks written (OTA below) |
| Reset matrix, Bluetooth advertising, project running | **24 / 24 clean**: 12 RTS (`rst:0x15`), 12 requested reboots (`rst:0x3`) |
| OTA X → Y | trial, confirmed on its link, engine installed, `[LOADER] core @0x1f0000 (proven)` |
| OTA X → a core that dies on trial | `(trial)` → `[LOADER] core @0x18000 (rolled back)`, the build refused from then on |
| Power cut mid-core (`board power-cycle`) | the core resumed at 561,152 B, finished, proven |
| LP analog I2C clock gated by hand (`LPPERI_CLK_EN = 0x5f000000`) | boots to the engine, no restore (`Saved PC` in the ROM, not in any bootloader) |
| A stage that spins at its entry, then `espflash write-bin` of ours | recovered over USB, no hands; boots ours |
| Door swap by USB, project kept | IDF → ours and ours → IDF, `0x0..0x8000` only, `basic` still lit both ways |
| Wi‑Fi, Bluetooth | joined the bench AP (DHCP, `lp-5490.local`); `[ble] advertising` |

Not checked on CX1: a register dump at core entry (V2: the emulator's
both-door state diff stands in), brownout behaviour (V12: nothing changed —
the same `LP_ANA` writes), ESP-NOW, the three-cold-retries rule on silicon
(emulated S4 passes through both doors).

**Found on silicon, not in the emulator:** after an RTS reset (`rst:0x15`)
a stage that spins at its entry spun for 10 s with no flash-boot watchdog
reset; after a power-on it looped on `TG0_WDT` as expected. The emulator
re-arms MWDT0's flash-boot protection after the USB-UART reset too. A
fidelity defect is filed. It does not change the contract: our stage arms
the RWDT (9 s) before it jumps, so a core that hangs early dies warm on
either kind of reset.

## Recovery

A bad second stage cannot be fixed over the air. The ROM's USB download
console is reached by the host's DTR/RTS dance before any of our code runs,
as long as no eFuse disables download mode (we never burn one) — proven on
the emulator with a stage that spins at its entry
(`lp-emu-esp32c6/tests/second_stage.rs`). The CLI flash path recovers;
Studio's recovery of a boot-looping native-USB board is an open defect
(`2026-09-02-studio-flasher-cannot-recover-a-boot-looping-c6`). Hands
fallback: BOOT held through a replug.

## Consequences

- Boot: power-on → core entry drops by the IDF's ~135 ms (emulated t3 and
  CX1 numbers in the PR and the planning dir's results).
- The analog-master wedge class is gone for every board that takes our
  stage: a gated LP analog clock boots (emulated: `second_stage.rs`).
- The `build.rs` linker-script patches, the esp-hal fork's `links` key and
  the factory loader's placeholder segments stay while any IDF-door path
  loads a monolithic image or the factory loader (dev images, fielded
  boards). Removing them is the plan's M7.
- Every core must keep booting through both doors: the gate is the
  emulator's split-boot test on every split image.
- `lpa_devices::bootloader`'s `Saved PC` ranges and the packaging guard name
  the IDF's code; ours lives in the loader's RAM and needs no flasher
  workaround.

## Amendment to 2026-10-04 (split image)

§2's `0x0` row: the second stage is ours (`bootloader.bin`, `LPLB` v1) on
every board flashed over USB from the cutover on; the IDF bootloader on
boards that have only been updated over the air. §3 (the loader) holds for
both flavours: the bootloader flavour is the same boot policy with a
prologue before it.
