# The C6 emulator's flash time (`t4`), against every CX1 storage record

**Date** 2026-10-17 · **Plan** `lp2025/2026-10-16-0848-tree-store-adoption`
(`emu-flash-timing.md`) · **Emulator** `lp-emu:esp32c6:t4` at `e4684bd42`
(this branch) · **Silicon** CX1 `c6-expendable`, `14:C1:9F:E6:54:90`, flash
JEDEC `0x464016`, records named per row · No board was touched for this report.

**The question.** Below `t4` an emulated flash command takes no time, so an
emulated mount, commit or format is CPU time only
(`docs/debt/emulated-flash-commands-take-no-time.md`). How close does an
emulator that times the flash interface come to the silicon records, run on
the firmware each record was taken on?

**The answer.** Mount, format, the flash-tears write and erase, and the
QIO/DIO difference land within ~15 % of CX1, most within 5 %. First light
lands 8–11 % fast — CPU, not flash. Request round trips (save, append, read,
load) do not: they are bound by the server's tick and the link, and the
emulator still differs there with both signs. `t4` closes the flash share of
them; what is left is not flash.

## What `t4` is

`lp-emu-esp32c6/README.md`, "What a flash command costs (`t4`)". In short: SPI1
transfers take their wire time at the SCK and line widths the registers give;
a program or erase holds WIP for CX1's busy times while the ROM's own loop
polls it; a cache fill is derived from SPI0's registers; a range invalidate
drops lines; SPI1 register accesses carry a fitted MSPI cost. `t1`, `t2` and
`t3` do not move.

**The clock, found in the registers.** Run in the emulator, the mask ROM and
espflash's bundled IDF bootloader (v5.1-beta1) leave `PCR.mspi_clk_conf =
0x500` (the 480 MHz SPLL ÷ 6) and `SPI0/SPI1.clock = 0x8000_0000` (one
division): **an 80 MHz SCK**, for a header that says 20, 40 or 80 MHz alike.
Only the ROM's own load of the bootloader runs at the header's divider. The
mode is the header's (QIO, QOUT, DIO and DOUT each leave their own line
widths). The same day's CX1 sitting (PR #1170) read the same registers off the
board. A DIO line fill at 80 MHz is 152 SCK = 304 CPU cycles; the silicon
fill `t3` was calibrated on is 338, so the per-transfer overhead is 34. That is
also why the `t3` calibration's 40 MHz arithmetic (608) was refuted.

## The table

Ratios are emulated ÷ silicon. Medians on both sides.

| figure | firmware | silicon | `t1` | `t3` | **`t4`** | `t4` ÷ si |
|---|---|---:|---:|---:|---:|---:|
| DIO line fill | `cycle-probe` / #1170 | 2.11 / 2.17 µs | — | 2.11 µs | **2.11 µs** | 0.97–1.00 |
| QIO line fill (predicted, app-side switch) | #1170 | 1.32 µs | — | — | **1.26 µs** | 0.96 |
| 256 B write (8 ROM programs) | `flash-tears` | 304–369 µs (min/max medians 318/353) | 26 | 50 | **340–343 µs** | ~1.0 (fitted) |
| 16 × 256 B writes (`program_us`) | `flash-tears` | 5,898 µs | — | 1,248 | **5,886 µs** | 1.00 |
| 4 KiB sector erase (`erase_us`) | `flash-tears` | 26.7 ms (21.1–32.6) | 0.005 | 0.012 | **21.0 ms** | 0.79 |
| first-boot format, 176 sectors | `55422b0e8` | 3,884 ms | 68 | — | **3,957 ms** | 1.02 |
| lazy first-boot format | `c20bf9a40` | 62.9 ms | — | 13.1 | **64.5 ms** | 1.03 |
| mount, c40 (as uploaded) | `55422b0e8` | 92.7 ms | 52.7 | — | **89.6 ms** | 0.97 |
| mount, 9 stores, cache-mapped reads (K1+) | `k12n` | 3.9–63.4 ms | — | 0.97–1.43 | **0.97–1.04** (empty 1.30) | see below |
| mount, 9 stores, cache-mapped, no index | `k12` | 3.8–223 ms | 0.66–0.70 | 0.95–1.43 | **0.94–1.00** (empty 1.31) | |
| mount, 9 stores, ROM-call reads | `pr1` | 7.9–290 ms | 0.42–0.60 | 0.76–0.82 | **0.99–1.04** (empty 1.22) | fitted here |
| mount, 9 stores, ROM-call reads | `pr1n` | 8.7–129 ms | 0.40–0.47 | 0.64–0.76 | **1.07–1.11** (empty 1.13) | independent |
| mount, 8 stores, DIO | #1170 `read` | 13.8–71.1 ms | — | — | **0.86–1.03** | |
| mount, 8 stores, QIO (app switch) | #1170 `qio` | 12.2–63.7 ms | — | — | **0.88–1.05** | |
| QIO ÷ DIO mount, 8 stores | #1170 | 0.851–0.896 | — | — | **0.861–0.908** | within 0.04 |
| first light, basic, fresh store | `k12n` split | 663 ms | — | 596 | **599 ms** | 0.90 |
| first light, c40, fresh | `k12n` split | 681 ms | — | 611 | **614 ms** | 0.90 |
| first light, playful-choker-tryout | `k12n` split | 706 ms | — | 623 | **626 ms** | 0.89 |
| first light, basic on a used store | `k12n` split | 690 ms | — | 632 | **635 ms** | 0.92 |
| push c40 (132 files) and load | `55422b0e8` | 3,273 ms | 1,145 | — | **2,827 ms** | 0.86 |
| save, round trip | `55422b0e8` | 235.8 ms | 77.1 | — | **171.6 ms** | 0.73 |
| save less a read of the same document | `55422b0e8` | 185.2 ms | 40.2 | — | **95.1 ms** | 0.51 |
| 4 KiB append, round trip | `55422b0e8` | 199.4 ms | 78.4 | — | **173.1 ms** | 0.87 |
| read the largest c40 file, round trip | `55422b0e8` | 54.7 ms | 36.6 | — | **75.9 ms** | 1.39 |
| whole-project load (stop + LoadProject) | `55422b0e8` | 233.8 ms | 73.5 | — | **159.1 ms** | 0.68 |

Sources: `flash-tears` silicon transcripts under
`lp-emu/transcripts/esp32c6/flash-tears/` (510 boots; the emulated run is one
boot of this tree's `test_flash_tears` image, ROM-up); the K1 mount sittings
(`results-k1/cx1/mount{2,3}`, images and ELFs from the K1 build); #1170's
DIO/QIO sitting (`results-qio/cx1/mount-nobench`, its `nobench` ELFs); the
K1 first-light sitting (`results-k1/cx1/first-light`, its `split-k12n` image);
the M7 perf sitting (`docs/reports/2026-10-15-tree-store-cx1-perf/`, firmware
`55422b0e8` rebuilt here with `heap_peak_diag`) and A3's (`c20bf9a40`). The
emulated runs are `emu_tree_store_mount`, `emu_tree_store_perf` and
`emu_first_light` with `LP_EMU_GRADE` / `LP_FIRST_LIGHT_GRADE` and the
records' own images.

### Mount, store by store (`t4`)

| store | `k12n` si / emu | `k12` si / emu | `pr1` si / emu | `pr1n` si / emu |
|---|---|---|---|---|
| empty-176 | 3.9 / 5.1 | 3.8 / 5.0 | 7.9 / 9.6 | 8.7 / 9.8 |
| c40-176 | 13.1 / 13.7 | 21.1 / 21.2 | 32.6 / 33.8 | 24.3 / 26.8 |
| used-176 | 29.0 / 29.5 | 128.7 / 123.3 | 164.9 / 164.7 | 64.5 / 71.3 |
| soak-176 | 23.8 / 23.5 | 98.5 / 94.7 | 125.7 / 125.8 | 49.1 / 54.7 |
| full-176 | 42.6 / 42.5 | 89.8 / 86.2 | 116.2 / 118.2 | 70.4 / 75.0 |
| panel-176 | 63.4 / 61.5 | 223.1 / 210.4 | 289.8 / 288.1 | 129.2 / 139.3 |
| used-128in176 | 28.3 / 28.5 | 98.7 / 94.4 | 128.9 / 129.7 | 58.5 / 64.1 |
| full-128in176 | 33.3 / 33.7 | 66.9 / 65.1 | 88.2 / 90.3 | 55.1 / 59.7 |
| panel-128in176 | 44.1 / 43.5 | 156.5 / 148.2 | 205.0 / 205.1 | 93.1 / 101.3 |

## What was fitted, and what was not

- **Fitted** (each named in the code with its source):
  - the per-transfer overhead (34 cycles), to the one measured DIO fill;
  - the 32-byte program's busy time (24 µs), to `ft-timing`'s write;
  - the SPI1 register access cost (+29 cycles over the APB's 9), to the
    `pr1` mounts — before it, `t4` read those 13–22 % fast, and the gap over
    the empty mount per extra SPI1 register access was 23–30 cycles on all
    five stores (per transfer it scattered more: 1.9–2.8 µs).
- **Not fitted**, and so the test: the SCK and line widths (registers), the
  sector erase (the format figure, then checked against `ft-timing`), the QIO
  prediction, `k12`/`k12n`/`pr1n` mounts, the format, first light, the
  round trips.

## Where it is not within 15 %, and why

- **The erase of a heavily cycled sector** (`ft-timing`, 0.79). One number for
  every erase; CX1's erase time varies 21–33 ms with the sector's wear (the
  scratch sector had been erased ~500 times) and content. The format's blank
  sectors erase in ~22 ms. A wear/content model would need a per-sector erase
  count and a sitting that varies it.
- **Empty-store mounts** (1.13–1.31, +1–1.7 ms). Present at `t3` too, so it is
  not flash; a few-millisecond mount is dominated by cold code fills and the
  first `[FS]` work. Small in absolute terms; not chased.
- **Request round trips** (save 0.73, its commit share 0.51, read 1.39, load
  0.68). Every round trip is quantized by the server's tick and carried by
  the link: an emulated read with no flash write in it takes 1.4× silicon's,
  while a save takes 0.73×. The flash share is now there (`t4` − `t3` on the
  `c20bf9a40` save is +20 ms for its 89 programs and their transfers); the
  rest is how the emulated link and server tick line up with the request,
  which this change does not touch. Quote these only image against image.
- **First light** (0.89–0.92). The boot path writes no flash on these boots,
  so `t4` adds ~3 ms to `t3`; the gap is `t3`'s known one-signed CPU residual
  (radio 65 against 84–87 ms, `frame1`, `lit`).
- **Not run:** the `panel-176` store's first light (the emulated boot lit
  nothing in 64 frames after the basic push, at `t3` as at `t4`; the silicon
  case lit at 738 ms — a separate difference, not flash); block erase and
  status-write times (no silicon figure).

**Checked for overlap:** `2026-09-10-the-emulated-c6-builds-a-graphics-stage-40x-slower-than-silicon.md`
is a guest that retires 1.3 billion instructions building a graphics stage —
instructions, not flash time. `t4` only adds time, so it cannot narrow it; it
stays open as it was.

## What moved

Nothing pinned: no test or figure runs at `t4`, and `t1`–`t3` are untouched
(the `harness_parity` memory replays pass at `t3` and `t4`). The usr engine
now executes the `0x01`/`0x31` status writes an app-side QIO switch sends
(before, it refused them and the switch failed); no shipped image sends them.
