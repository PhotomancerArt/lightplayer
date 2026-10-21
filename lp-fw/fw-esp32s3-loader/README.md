# fw-esp32s3-loader

The ESP32-S3's **split-image loader**: the small app the IDF second-stage
bootloader starts from the first bytes of the `factory` partition. It reads the
two boot records, picks a core, maps it, copies its RAM segments and jumps.
It runs entirely from RAM (about 3 KB) and is the one piece of the image that
is **never updated over the air**: changing it takes a USB flash. It is the C6
loader ([`fw-esp32c6-loader`](../fw-esp32c6-loader/README.md)) on the S3's
machine, with the C6's policy, words and rules.

```text
the IDF door:  ROM → IDF bootloader (0x0) → loader (0x10000) → core → engine
```

Only the IDF door exists on the S3 (espflash's bundled S3 bootloader at
`0x0`). LightPlayer's own second stage, as the C6 has, is future work; the
crate is shaped so that flavour adds a feature, a linker script and a
prologue in front of `loader_main` without changing the loader's job.

The layout is **layout 2** ([`lp-bootctl`](../../lp-base/lp-bootctl/README.md),
`split_layout_2`): layout 1's loader, progress record and boot records at the
same offsets, `0x18000..0x20000` reserved, the region from `0x20000` on the
S3's 64 KiB MMU pages. The records, `choose` and the marks are the C6's,
shared through `lp-bootctl`, so the loader and the core cannot disagree about
which record wins.

## What it does

1. Reads both boot-record sectors and classifies the reset (cold or warm,
   `lp_bootctl::ResetKind::from_s3_reason` on the ROM's
   `rtc_get_reset_reason(0)`). `lp_bootctl::choose` picks the newest valid
   record, or the one before it when the newest is a trial that failed.
2. Walks the chosen core's ESP image, mapping no more flash than the record's
   `core_len`: flash segments are mapped at their link addresses through the
   MMU, RAM segments are copied through the scratch window.
3. Falls back to the other record's core when the chosen one does not load,
   and says why.
4. Unmaps its scratch window, invalidates both caches and jumps to the core's
   entry point.

One ROM line is the only output before the core runs, for example
`[LOADER] core @0x20000 (proven)` — or `trial`, `rolled back`, `fallback`,
`no record`. A core that does not load is one line too:
`[LOADER] core @0x… skipped: <why>`. The loader carries a version word
(`LPLV` v1, `lp_bootctl::loader_identity`) that a core and `lp-fw-split` find
by scanning the loader's first 4 KiB.

## The MMU, and what the loader claims

The S3 has one MMU table of 512 entries, one 64 KiB page each, serving the
data bus (`0x3C00_0000 + n·64 KiB`) and the instruction bus
(`0x4200_0000 + n·64 KiB`) at the same index. The plan is
`lp_bootctl::s3_mmu`. Because the loader is never updated, what it assumes
binds every core it will ever boot (ruling L4):

- **entry 0 is the core's first page**;
- **a core's flash segments stay below entry `0x1C0`**: the loader refuses a
  segment that reaches its scratch window;
- **entries `0x1C0..0x200` are the loader's scratch**, through which it reads
  the records and the core's image, and every one of them is unmapped before
  the jump;
- **its RAM, `0x3FCB_0000..0x3FCC_0000`** (data view, `lp_bootctl::S3_LOADER_RAM`),
  is inside every core's `.bss`.

## Where a segment may go

- Flash, either bus: mapped, page-congruent, below the scratch window.
- Data RAM, RTC fast and slow RAM: copied.
- SRAM1's instruction view (`0x4037_8000..`): copied through its data-bus
  alias (`−0x6F_0000`), where byte stores work.
- SRAM0's instruction RAM (`0x4037_0000..0x4037_8000`, no data-bus alias):
  copied with 32-bit stores only, which is all instruction RAM takes.
- Never over either boot stage's RAM (`lp_bootctl::S3_BOOT_STAGE_RAM`): the
  loader's own, and the IDF bootloader's, whose stack the loader runs on.
- Never address 0, and nowhere outside RAM: refused by name.
  espflash's image builder fills a gap before a flash segment with an
  address-0 segment, which the IDF bootloader skips; the C6 loader once
  copied one to address 0 and faulted
  (`docs/defects/2026-10-14-the-c6-loader-copies-an-address-zero-padding-segment-to-address-zero.md`).

## The two placeholder segments

The IDF bootloader maps an app's flash segments and asserts the map it
computed is page-aligned; with no flash segment at all, espflash 3.3's bundled
S3 build `abort()`s at `0x403ceab9` on zeroes it computed. So the loader
carries two 16-byte placeholders in **one** flash page (`loader.x`): the data
view's at `0x3C00_0020` (it carries the version word) and the instruction
view's at `0x4200_0120`, both MMU entry 0. The loader never reads them; it
remaps entry 0 for the core.

## The RAM it lives in

Code runs from the instruction-bus view and data from the data-bus view of
the same SRAM1: code at I `0x403A_0000` (= D `0x3FCB_0000`), data at D
`0x3FCB_8000`, written into the linker script by `build.rs` from
`lp_bootctl::S3_LOADER_RAM` so the number has one home. That is inside every
core's `.bss` (the core zeroes it after the jump) and below the IDF
bootloader's own RAM (`lp_bootctl::S3_IDF_BOOTLOADER_RAM`, D `0x3FCD_9700`
and up), which is still live while the loader runs: the loader runs on the
stack the IDF bootloader called it on. `.bss` rides in `.data` as zeroes,
because the IDF bootloader loads an image's file bytes only.

## What it never does

- **It never writes flash.** The core marks a trial attempted, started and
  confirmed; the loader only reads.
- **No ROM SPI1 flash routines.** Reading flash goes through the cache and
  the scratch window (`src/flash_window.rs`): on the C6, an earlier ROM flash
  access left the core's flash driver reading garbage.
- **It reads no partition table.** The records say where the core is.
- **Keep it small and boring.** Anything the core can do, the core does,
  because the core can be updated and the loader cannot.

## Building

A standalone crate with its own workspace, target (`xtensa-esp32s3-none-elf`,
the `esp` toolchain) and linker script, excluded from the root workspace:

```bash
(cd lp-fw/fw-esp32s3-loader && PATH="$(just _xt-gcc-dir):$PATH" cargo build --release)
```

You normally do not build it by hand: `lp-fw-split build --chip esp32s3`
builds the whole split image, this loader included, through
[`tools/lp-fw-split`](../../tools/lp-fw-split/README.md). The image must stay
within `lp_bootctl::LOADER_MAX_LEN` (`0x5000`).
