/* SPIKE: the S3 split-image loader lives entirely in internal SRAM1.

   Code runs from the instruction-bus view, data from the data-bus view of
   the same SRAM (I 0x403A_0000 = D 0x3FCB_0000, the +0x6F_0000 alias).
   Both lie inside the core's .bss (0x3FC8_C128..0x3FCC_E8C8 on the
   spike's image) — never inside a segment the loader copies — and below
   the IDF bootloader's own RAM (≈ 0x3FCD_9700 and up, D view), which is
   still live while this image is loaded. The core zeroes .bss over the
   loader after the jump. */
MEMORY {
  LIRAM : ORIGIN = 0x403A0000, LENGTH = 0x8000
  LDRAM : ORIGIN = 0x3FCB8000, LENGTH = 0x8000
}
ENTRY(_start)

PROVIDE(ets_printf = 0x400005d0);
PROVIDE(rtc_get_reset_reason = 0x4000057c);
PROVIDE(Cache_Invalidate_ICache_All = 0x400016d4);
PROVIDE(Cache_Invalidate_DCache_All = 0x400016e0);

SECTIONS {
  .text : ALIGN(4) {
    KEEP(*(.literal._start .text._start))
    *(.literal .text .literal.* .text.*)
    . = ALIGN(4);
  } > LIRAM
  /* The IDF bootloader loads file bytes only: .bss rides in .data as
     zeroes so nothing reads stale RAM. */
  .data : ALIGN(4) {
    KEEP(*(.loader_identity))
    *(.rodata .rodata.*)
    *(.data .data.*)
    *(.bss .bss.* COMMON)
    . = ALIGN(4);
  } > LDRAM
  /DISCARD/ : { *(.eh_frame*) *(.xtensa.info) *(.comment) }
}
