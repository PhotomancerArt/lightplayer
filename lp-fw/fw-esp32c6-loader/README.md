# fw-esp32c6-loader

The ESP32-C6's **split-image loader**: the small app the IDF second-stage
bootloader starts from the first bytes of the `factory` partition. It reads the
two boot records, picks a core, maps it, copies its RAM segments and jumps.
It runs entirely from RAM (about 2.8 KB) and is the one piece of the image that
is **never updated over the air**: changing it takes a USB flash.

```text
IDF bootloader → loader (0x10000) → core (0x18000, or the high end) → engine
```

The decisions, the byte layouts and the rules found on silicon are in
[`docs/adr/2026-10-04-c6-split-link-firmware-loader-and-boot-records.md`](../../docs/adr/2026-10-04-c6-split-link-firmware-loader-and-boot-records.md).
The formats live in [`lp-bootctl`](../../lp-base/lp-bootctl/README.md), shared
with the core, so the loader and the core cannot disagree about which record
wins.

## What it does

1. Reads both boot-record sectors and classifies the reset (cold or warm,
   `lp_bootctl::ResetKind`). `lp_bootctl::choose` picks the newest valid
   record, or the one before it when the newest is a trial that failed.
2. Walks the chosen core's ESP image, mapping no more flash than the record's
   `core_len`: flash segments are mapped at their link addresses through the
   MMU, RAM segments are copied through a scratch mapping.
3. Falls back to the other record's core when the chosen one does not load,
   and says why.
4. Invalidates the flash cache and jumps to the core's entry point.

One ROM line is the only output before the core runs, for example
`[LOADER] core @0x18000 (proven)`. The loader carries a version word
(`LPLV`, `lp_bootctl::loader_identity`) that a core finds by scanning the
loader's first 4 KiB, to know what the loader in front of it can do.

## Rules (each one found on silicon)

- **It never writes flash.** The core marks a trial attempted, started and
  confirmed; the loader only reads.
- **No ROM SPI1 flash routines.** Reading flash goes through the MMU window
  (`src/flash_window.rs`). esp-storage sizes the part with an `RDID` on SPI1,
  and any earlier ROM flash access left that probe returning garbage.
- **It reads no partition table.** The records say where the core is.
- **Keep it small and boring.** Anything that could be done by the core is done
  by the core, because the core can be updated and the loader cannot.

## Building

A standalone crate with its own workspace, target and linker script, excluded
from the root workspace. You normally do not build it by hand:
`just fw-esp32c6-split` builds the whole split image (two link passes, the
verifier, the loader and the merged image) through
[`tools/lp-fw-split`](../../tools/lp-fw-split/README.md), and
`just fw-esp32c6-size-check` gates its headroom.
