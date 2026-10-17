# C6 flash mode vs first light and mount

**Date** 2026-10-17 · **Plan** `lp2025/2026-10-16-0848-tree-store-adoption`
(K1+ follow-up) · **Board** CX1 `c6-expendable` (MAC `14:C1:9F:E6:54:90`),
silicon, leased · **Firmware** main `a67e8229e` (#1160 and #1161 merged; the
same tree as `0f01e0d78`), plus the experiment on branch
`claude/exp-c6-flash-mode` (never to merge) · **Records**
`results-qio/` in the planning directory.

**The question.** The C6 image is labelled "DIO 40 MHz". Would QIO, or DIO at
80 MHz, or QIO at 80 MHz, speed up every flash read: the code cache, the
bootloader, and the tree store's mount?

**The answer.**

1. **The app already runs the flash at 80 MHz, DIO.** "40 MHz" is only the
   image header. The bootloader prints it, and the ROM and bootloader run at
   that speed. Once esp-hal moves the CPU to the PLL, the flash runs at
   480 MHz ÷ 6 = 80 MHz (registers below). A 32-byte cache line fill measures
   **2.17 µs**, which is about 152 clocks of a DIO read at 80 MHz. At 40 MHz it
   would be about 3.8 µs. **So "DIO 80 MHz" is today's setting, and there is
   nothing to gain from it.** Setting the header to 80 MHz
   (`espflash --flash-freq 80mhz`) changes nothing measurable.
2. **QIO works on CX1 when the app turns it on**, and it is a real gain. A line
   fill drops from 2.17 to **1.32 µs** (−39 %), and 1 MiB read through the cache
   drops from 71.0 to 44.0 ms (+61 % throughput). **First light** on a fresh
   store drops by **90–99 ms** (basic 654 → **564 ms**, playful-choker 669 →
   570, c40 666 → 576). Most of the gain is code-cache refills in `lit`,
   `frame1`, `load` and `radio`. **Mount** drops by 10–14 %.
3. **QIO cannot come from the image header with today's bootloader.** The
   bootloader espflash bundles (ESP-IDF v5.1-beta1) was built without QIO
   support. Worse, a QIO header makes the **ROM** load the bootloader in QIO, and
   on CX1 that fails (`mode:QIO, clock div:2` … `ets_loader.c 67`). The board
   then boots nothing until it is reflashed over USB. **Never ship a QIO
   header.**
4. **Idle cleanup changes which store state is worst.** The panel-heavy
   history behind the 44/63 ms figures was written with **no idle steps at
   all**. Written the way a board writes it, with `collect_step` in the quiet
   gaps (#1153), it mounts in **14–16 ms** (DIO) or **13–14 ms** (QIO) at both
   128 and 176 sectors, and it is inside the bar. The worst realistic state is
   now a **full** store. That mounts in 35.5 / 46 ms (DIO) or 31.6 / 41 ms
   (QIO) at 128 / 176, so it is still over ≤ 25 / ≤ 30 ms.

## The table

### Flash reads (diag images, 100 power-on boots per mode, all identical to the µs)

| mode | line fill (8,192 × 32 B, 8× the cache) | 1 MiB through the cache | 1 MiB checksum before vs after the switch |
|---|---:|---:|---|
| DIO, 80 MHz (today) | 2,167 ns | 71.0 ms (14.8 MB/s) | equal, 100/100 |
| QIO, 80 MHz (app switches) | **1,315 ns** | **44.0 ms (23.8 MB/s)** | equal, 100/100 |

### First light (`[perf] first light`, split `fs-tree` image, 10 power-on boots each, median ms)

| case | DIO total | QIO total | Δ | DIO → QIO parts (`radio` · `load` · `frame1` · `lit`) |
|---|---:|---:|---:|---|
| basic, fresh store | 654 (653–655) | **564** (564–567) | −90 | 84→76 · 89→68 · 88→63 · 189→157 |
| playful-choker-tryout, fresh | 669 (668–670) | **570** (569–574) | −99 | 83→76 · 161→124 · 103→78 · 115→90 |
| c40, fresh | 666 (665–687) | **576** (575–580) | −90 | 83→76 · 96→74 · 88→63 · 190→157 |
| basic on panel-heavy/176 | 722 (fs 54) | 614 (fs 50) | −108 | 87→79 · 90→69 · 87→64 · 204→156 |
| basic on full/176 | 702 (fs 44) | 604 (fs 40) | −98 | 87→78 · 90→69 · 92→64 · 190→156 |
| basic on used/176 | 682 (fs 27) | 582 (fs 23) | −100 | 86→78 · 90→69 · 88→59 · 190→157 |
| basic on panel-burst/176 (idle steps) | 668 (fs 14) | 576 (fs 13) | −92 | 87→78 · 90→68 · 88→63 · 190→157 |

`boot` is 167 ms in every row, both modes: the ROM, the bootloader's log, and
the loader. `init` 21 → 20, `split` 4 → 3, `server` 7 → 6. The QIO switch runs
inside `init`, takes 47 µs, and adds nothing measurable there. A
pre-written store's `fs` part depends on how much idle cleanup ran during the
test's own set-up boots, so compare those rows on the parts other than `fs`.
The mount table below compares like with like.

The core's own SHA-256 (1.45 MB read through the cache after first light) came
out equal to the boot record's in every hello in both modes. That is 62 QIO
boots and 42 DIO boots.

### Mount (`[perf] fs mount`, diag images, 10 power-on boots, each image written over `lpfs`; boot #0 / median, ms)

| store image | DIO #0 / med | QIO #0 / med | QIO ÷ DIO | bar |
|---|---:|---:|---:|---|
| c40/176 | 13.9 / 13.9 | 12.1 / 12.2 | 0.87 | ✓ |
| used/176 | 31.3 / 30.8 | 26.9 / 26.5 | 0.86 | ✓ QIO only |
| **full/128** (128in176) | 35.6 / 35.5 | **31.6 / 31.6** | 0.89 | ✗ (≤ 25) |
| **full/176** | 45.5 / 46.1 | **40.8 / 41.4** | 0.90 | ✗ (≤ 30) |
| panel-heavy/128, no idle | 52.0 / 50.3 | 45.5 / 44.1 | 0.88 | ✗ |
| panel-heavy/176, no idle | 74.5 / 70.5 | 65.9 / 63.2 | 0.90 | ✗ |
| **panel-burst/128**, idle steps | 15.6 / 14.5 | 13.7 / 12.7 | 0.88 | ✓ |
| **panel-burst/176**, idle steps | 15.4 / 14.3 | 13.5 / 12.6 | 0.88 | ✓ |
| panel-settled/128, idle steps | 13.9 / 13.9 | 12.3 / 12.3 | 0.88 | ✓ |
| panel-settled/176, idle steps | 13.8 / 13.9 | 12.3 / 12.2 | 0.88 | ✓ |

Two notes on reading this table:

- The panel and used rows fall from boot to boot (panel/176 DIO: 74.5 → 67.6
  over ten boots). That is the board's own idle cleanup at work: it runs while
  the board sits quiet between power cycles, and it forgets garbage-only
  sectors. So boot #0 is the image exactly as written. A second sitting, five
  boots per image on images without the pre-mount benchmark, read the same to
  within 1 % (`mount-nobench`).
- This sitting's DIO column reads 6–18 % above K1+'s figures for the same image
  bytes (K1+ reported panel 44 / 63 ms and full 33 / 43 ms). The firmware
  between the two sittings differs: K1+'s pre-merge head against main
  `a67e8229e`, which also carries #1153 and #1158. Both columns here use the
  same firmware, so the ratio between them is the flash mode alone.

### Header experiments (CX1, `espflash monitor`, one boot each)

| image header | ROM line | bootloader says | app registers | result |
|---|---|---|---|---|
| DIO, 40 MHz (today) | `mode:DIO, clock div:2` | SPI Speed 40MHz, Mode DIO | 80 MHz DIO | boots |
| DIO, 80 MHz | `mode:DIO, clock div:1` | SPI Speed 80MHz | identical | boots; bootloader timestamps identical to the ms |
| QIO | `mode:QIO, clock div:2` | — | — | **ROM fails to load the bootloader (`ets_loader.c 67`)**; recovered over USB |

## What the hardware is doing (registers, read at boot)

Today's image, before any switch:

- SPI0 (the cache's controller) `CLOCK = 0x80000000`: 1-division, so the bus
  runs at the MSPI module clock.
- PCR `MSPI_CLK_CONF = 0x500`: high-speed divider 5, which divides by 6.
  `SYSCLK_CONF = 0x28010200`: SPLL. So the bus runs at 480 ÷ 6 = **80 MHz**.
- SPI0 `CTRL = 0x00ac200c` (`fread_dio`, `fastrd_mode`), `USER2 = 0x700000bb`
  (command `0xBB`). That is DIO.

After the app's switch: `CTRL = 0x012c200c` (`fread_qio`), `USER2 = …eb`
(command `0xEB`), `USER1 = 0x7c000043` (a 32-bit address phase and 4 dummy
cycles). SPI1 follows the same way. The clock is unchanged.

K1+ estimated the line fill at "~150 SPI clocks ≈ 4 µs at 40 MHz". The clock
count is right, but the clock is 80 MHz, so the fill is ~2 µs. The ~9 µs per
store read that K1+ measured is therefore mostly the read path itself (the
call, the bounce buffer, and a header that straddles two lines), not the SPI
clock. That is why QIO trims the mount by only 10–14 % while it trims the
code-cache-bound parts of first light by 10–28 %.

## Flash part support

| board | chip (eFuse) | flash | QIO wiring | QE bit | evidence |
|---|---|---|---|---|---|
| **CX1** (generic Amazon C6) | **ESP32-C6FH4**: `PKG_VERSION 1`, `FLASH_CAP 1` (4 MB in-package), `FLASH_VENDOR 1` (XMC), `FLASH_TEMP 1` | JEDEC `46 40 16`: XMC's second manufacturer code (ESP-IDF `SPI_FLASH_XMC_2`), 4 MiB, in-package | inside the package (no WP/HD board wiring to get wrong) | **already set at the factory** (SR2 = `0x02`, every boot) | read on silicon this sitting |
| **XIAO ESP32-C6** (FC6, LC6, OAK) | **ESP32-C6FH4** (Seeed schematic v1.0, `XIAO_ESP32_C6_v1.0_SCH_260114.pdf`) | 4 MB in-package; part and vendor **not read**: the rules for this run allowed CX1 only, and the desk registry holds no XIAO JEDEC ids | inside the package | **unknown** | Arduino-esp32's `boards.txt` ships `XIAO_ESP32C6` at `flash_mode=qio`, `flash_freq=80m`, which is strong evidence the part runs QIO at 80 MHz |

CX1 turns out to be the same chip variant as the XIAO, an FH4 with in-package
flash, and not a module with an external flash chip. That makes it more
representative than its "generic Amazon board" label suggests. Until each
XIAO is read, though, the XIAO's flash vendor and its QE bit remain the open
facts.

## What controls what, and what reaches a fielded board

| knob | where it lives | what it does on the C6 | how it reaches a fielded board |
|---|---|---|---|
| header flash **frequency** (`--flash-freq`) | byte 3 of the bootloader's and the app's image headers | changes only the ROM's divider for its own load of the bootloader (21 KB). The bootloader never sets the clock, and the app runs at 80 MHz regardless | USB flash only. Not worth it: measured no change |
| header flash **mode** (`--flash-mode`) | byte 2 | a QIO header makes the ROM load the bootloader in QIO, which **fails on CX1**. The bundled bootloader has no QIO code to enable QE or switch modes | USB flash only. **Never QIO** |
| a QIO-capable **bootloader** (ESP-IDF built with `CONFIG_ESPTOOLPY_FLASHMODE_QIO`, its own header kept DIO) | `0x0` | switches to QIO before the loader runs. Only the bootloader's own reads gain (the 2.8 KB loader), and the core would still need nothing | USB flash only. Would need ESP-IDF on the desk, a provenance header, and a change to every flash path. **No benefit beyond the app-side switch** |
| **app-side switch** (this experiment) | the core, right after `init_board` | ROM `esp_rom_spiflash_config_readmode(QIO)` plus `esp_rom_spiflash_select_qio_pins(0, 0)`, run from IRAM with the cache suspended. The QE bit must be set (read SR2 with `0x35`) | **over the air: a core update.** No persisted format, no wire change, nothing at `0x0` |
| SPI clock above 80 MHz (HS divider 4 → 96, 3 → 120 MHz) | PCR | outside the C6's specified 80 MHz flash clock; ESP-IDF offers only 80 MHz on the C6 | not tried |

## Host model: panel-heavy with idle steps

`lp-tree-store` `mount_cost_tests::panel_idle_report` (experiment branch)
rewrites the panel-heavy history (c40, then 450-byte `panel.json` saves until
the partition has been written over three times) with the server's idle
cadence: steps only after 2 s quiet, one every 0.5 s (`fs_idle.rs`). Mount
reads (simulator counts):

| history | calls 128 / 176 | first-pass calls 128 / 176 | silicon mount, DIO #0 128 / 176 |
|---|---:|---:|---:|
| panel, **no idle** (the 44/63 ms history) | 3,810 / 5,250 | 3,518 / 4,958 | 52.0 / 74.5 ms |
| panel-burst: bursts of 20 saves, 10 s quiet (16 steps) between, ending mid-burst | 813 / 857 | 532 / 574 | 15.6 / 15.4 ms |
| panel-burst-quiet: the same, ending after the quiet | 726 / 770 | 445 / 487 | — |
| panel-slow: one save every 3 s (2 steps each) | 707 / 755 | 415 / 463 | — |
| panel-settled: steps until settled after every save | 707 / 755 | 415 / 463 | 13.9 / 13.8 ms |

**Yes, the 44/63 ms figures came from a history with no idle at all.** That
history needs a board that never had 2 s of quiet across three full rewrites of
its partition, which is not how a board is used. With idle cleanup, the
realistic worst panel-heavy mount is about c40's: **~15 ms (DIO) or ~13.5 ms
(QIO)**. Since #1166 (main `dd8534867`, merged during this run) paces the idle
steps by frame slack while a show plays. With a show running, steps may come
less often than this model's cadence, so the burst row is the one to plan
around, not the settled row.

## Risks

- **The XIAO is unread.** CX1 is an FH4 with XMC in-package flash and QE set at
  the factory. An XIAO batch could carry another vendor's die. If its QE bit
  is clear, the switch must stay in DIO. It must never write the status
  register (see the next point).
- **Writing QE is a persistent, non-volatile status-register write.** A power
  cut during it could leave protection bits set. The proposal is
  read-only: **enter QIO only when SR2 already reads QE = 1, and otherwise stay
  DIO.** The experiment's write path (`0x31`, then a 16-bit `0x01`) never ran,
  because QE was already set every time.
- **A boot that fails after the switch fails every boot.** Over the air that is
  survivable: an update is a trial, so the loader gives the previous core back.
  A core flashed over USB has no rollback.
- **The ROM calls are marked private** ("do not call in SDK"). ESP-IDF's own
  bootloader makes the same two calls in the same order (`flash_qio_mode.c`),
  and the experiment verified the result register by register. A new chip
  revision's ROM would need a re-check.
- **RAM and flash cost.** The experiment's `switch` is 1,556 B of IRAM, with the
  status-write paths and the readout. The read-only form should be a few
  hundred bytes of IRAM, which comes out of the heap, plus a little core text.
  The core sits 2–3 KB under its next 32 KiB page (K1+), so a careless
  version could cost a whole page of headroom. The experiment build itself
  dropped the gated headroom to 47,420 B with its println readout. That build
  is not a candidate.
- **The emulator.** `lp-emu-esp32c6` decodes SPI1's `0xEB` and ignores bus
  width, and it would execute the real ROM's `config_readmode`. Whether every
  register that touches reads back sanely has not been walked. Run
  `just walk-esp32c6-emu` and the C6 gates before anything ships.
- **Integrity evidence, and its limits.** The evidence is 200 diag boots with
  1 MiB compared across the switch, 0 mismatches; 3 × 96 × 4 KiB files read
  back after power-on boots in each mode, 0 mismatches and no CRC lines;
  every QIO hello's core SHA-256 correct; and every mount clean. It is **one
  board** at desk temperature, with no soak and no temperature sweep.

## Recommendation for Yona

1. **Make the app-side QIO switch a real change, in its own plan.** It is
   read-only on QE, sits in the core right after `init_board`, uses no header
   and no bootloader, and keeps a log line that names the mode. It is worth
   **~90–100 ms of first light (−14 %)** and 10–14 % of every mount on CX1.
   That is the biggest single saving still on the table after #1155 and
   #1163, and it reaches fielded boards over the air. Before it ships, it
   needs:
   - one XIAO desk sitting that reads the JEDEC id, SR2 and the eFuse flash
     fields on FC6 and LC6;
   - an emulator walk;
   - a soak (hours of render with flash writes);
   - the size check.
2. **Do not touch the header.** 80 MHz already holds where it matters, and a
   QIO header bricks the boot with this bootloader. Correct the "DIO 40 MHz"
   wording in K1+'s notes: the C6 runs DIO at 80 MHz.
3. **Mount bar:** with idle cleanup counted, the worst realistic state is a
   **full** store, at 35.5 / 46 ms (DIO) or 31.6 / 41 ms (QIO) at 128 / 176.
   That is 6–11 ms over the ≤ 25 / ≤ 30 ms bar even with QIO. Either relax the
   bar for "full" (it is ≤ 7 % of a ~600 ms first light) or build the bounded
   checkpoint (C4s, additive, reserved by #1158). QIO alone does not close
   it.
