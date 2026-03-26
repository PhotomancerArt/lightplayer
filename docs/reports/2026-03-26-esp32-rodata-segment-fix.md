# ESP32 Boot Failure: rodata_desc Segment Split

**Date:** 2026-03-26
**Status:** Fixed — firmware boots on device
**Related:** [ESP32 Stack Unwinding Implementation](2026-03-13-esp32-unwinding-implementation.md) (Problem 5)

---

## Symptom

Firmware failed to boot on ESP32-C6:

```
Assert failed in unpack_load_app, bootloader_utility.c:762 (rom_index < 2)
```

Same assertion as Problem 5 in the unwinding implementation report.

## Root cause

esp-hal's `rodata.x` linker script defines `.rodata_desc` and `.rodata` as
**separate output sections**:

```ld
SECTIONS {
  .rodata_desc : ALIGN(4) {
    KEEP(*(.rodata_desc));
  } > RODATA

  .rodata : ALIGN(4) {
    *(.rodata .rodata.*)
  } > RODATA
}
```

`.rodata_desc` is 256 bytes (ESP app descriptor). `.rodata` has 128-byte
alignment inherited from input sections. This creates a 96-byte alignment gap
between them:

```
.rodata_desc  0x42000020  size=0x100  (ends at 0x42000120)
              0x42000120  96 bytes alignment padding
.rodata       0x42000180  size=0x36cf0
```

The ELF linker merges both into a single LOAD segment (correct). However,
`espflash` creates ESP image segments from **ELF sections**, not LOAD segments.
It sees the gap between `.rodata_desc` and `.rodata` and splits them into
separate image segments:

```
image seg 0: vaddr=42000020 size=00100 (map)    ← .rodata_desc
image seg 2: vaddr=42000180 size=5bdec (map)    ← .rodata + .gcc_except_table.*
image seg 4: vaddr=4205bf90 size=1c81e4 (map)   ← .text + .eh_frame
```

Three ROM-mapped segments. The ESP32 bootloader supports at most 2.

### Why this wasn't an issue before

Previously, `.rodata` input sections had smaller alignment (4 or 16 bytes),
so the gap between `.rodata_desc` and `.rodata` was small enough that espflash
treated them as contiguous. The addition of Cranelift and its dependencies
introduced input `.rodata` sections with 128-byte alignment, widening the gap
beyond espflash's tolerance.

## Fix

Same pattern as the `.eh_frame` fix from Problem 5: merge everything into a
single output section definition. The `build.rs` now also patches `rodata.x`:

```ld
SECTIONS {
  .rodata : ALIGN(4)
  {
    KEEP(*(.rodata_desc));
    KEEP(*(.rodata_desc.*));
    . = ALIGN(4);
    _rodata_start = ABSOLUTE(.);
    *(.rodata .rodata.*)
    *(.srodata .srodata.*)
    *(.gcc_except_table .gcc_except_table.*)
    . = ALIGN(4);
    *( .rodata_wlog_*.* )
    . = ALIGN(4);
    _rodata_end = ABSOLUTE(.);
  } > RODATA
}
```

This places `.rodata_desc`, `.rodata`, `.gcc_except_table.*`, and
`.rodata.wifi` all inside one output section. The alignment padding becomes
internal to the section rather than a gap between sections.

Additionally, the `.gcc_except_table.*` sections (hundreds of them from
`panic=unwind` LSDA data) are now explicitly captured in the `.rodata` section
definition. Previously they were placed by linker default rules and happened to
be contiguous, but explicit capture is more robust.

### Result

```
LOAD  0x42000080  R     (.rodata)              ← 1 ROM segment
LOAD  0x4205bf90  R E   (.text + .eh_frame)    ← 1 ROM segment
```

Section count dropped from 802 to 22 (the hundreds of individual
`.gcc_except_table.*` sections merged into `.rodata`). Binary size unchanged.

## Files changed

- `lp-fw/fw-esp32/build.rs` — patches `rodata.x` in addition to `text.x` and
  `eh_frame.x`

## Lessons learned

1. **espflash uses ELF sections, not LOAD segments, to create image segments.**
   The ELF can have correct LOAD segments while espflash still creates too many
   image segments due to gaps between sections within a LOAD segment.

2. **Input section alignment propagates to output sections.** Adding a
   dependency with 128-byte aligned `.rodata` widened an alignment gap that was
   previously harmless. Linker script changes are needed to absorb alignment
   padding inside a single output section.

3. **All ROM content should be in exactly 2 output section definitions:**
   one for `.rodata` (R) and one for `.text` (R E). Any additional output
   section in ROM risks creating a 3rd image segment.
