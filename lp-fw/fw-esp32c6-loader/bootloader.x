/* LightPlayer's own second-stage bootloader (the `bootloader` flavour of
   this crate): loaded by the mask ROM from flash 0x0, run entirely from RAM.

   It takes the same RAM as the factory loader, 0x40850000..0x40860000:
   - below everything the ROM keeps live while it loads this image (its
     shared buffers from 0x4086AD08, its stack 0x4087C610..0x4087E610, its
     data above);
   - above every RAM segment of a core (they end below 0x40820000; the
     loader refuses one that overlaps, `LOADER_RAM` in main.rs);
   - inside the core's .bss/stack, which the core zeroes after the jump.
   No flash placeholders: nothing here asserts a count of mapped segments.
   lp-fw-split writes the image (header, segments, checksum, SHA-256). */
MEMORY {
  LRAM : ORIGIN = 0x40850000, LENGTH = 0x10000
}
ENTRY(_start)

PROVIDE(ets_printf = 0x40000028);
PROVIDE(rtc_get_reset_reason = 0x40000018);
PROVIDE(Cache_Invalidate_ICache_All = 0x4000064c);
PROVIDE(Cache_Suspend_ICache = 0x40000698);
PROVIDE(Cache_Resume_ICache = 0x4000069c);

SECTIONS {
  .text : ALIGN(4) {
    /* "LPLB", version: first, so it lands at image offset 0x20. */
    KEEP(*(.bootloader_identity))
    KEEP(*(.text.entry))
    *(.text .text.*)
    *(.rodata .rodata.* .srodata .srodata.*)
    *(.data .data.* .sdata .sdata.*)
    . = ALIGN(4);
  } > LRAM
  .bss (NOLOAD) : ALIGN(4) {
    _bss_start = .;
    *(.bss .bss.* .sbss .sbss.* COMMON)
    . = ALIGN(4);
    _bss_end = .;
  } > LRAM
  _bootloader_end = .;
  _stack_top = ORIGIN(LRAM) + LENGTH(LRAM);
  /DISCARD/ : { *(.eh_frame*) *(.riscv.attributes) }
}
