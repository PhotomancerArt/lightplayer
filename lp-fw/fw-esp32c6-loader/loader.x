/* The split-link loader lives entirely in RAM: it remaps the flash window
   at 0x42000000 under the core's link address, so it must not run from it.

   0x40850000.. sits inside the core's .bss/stack range — never inside a
   segment the loader copies (the core's loaded segments end below
   0x40820000) — and well clear of the IDF bootloader's own RAM
   (0x4086C410.. and up), which is still live while this image is loaded.
   The core's startup zeroes .bss over the loader after the jump. */
MEMORY {
  LRAM : ORIGIN = 0x40850000, LENGTH = 0x10000
  /* The IDF bootloader asserts an app has exactly two flash-mapped
     segments (its rodata and its text: `rom_index == 2`). The loader has
     neither, so it carries two 16-byte placeholders, apart so the image
     tool keeps them as two segments, inside one page so the image stays
     small. The loader never reads them; it remaps this page for the core. */
  ROMSTUB : ORIGIN = 0x42000020, LENGTH = 0x200
}
ENTRY(_start)

PROVIDE(ets_printf = 0x40000028);
PROVIDE(Cache_Invalidate_ICache_All = 0x4000064c);

SECTIONS {
  .drom_stub : { LONG(0x4c504452) LONG(0) LONG(0) LONG(0) } > ROMSTUB
  .irom_stub 0x42000120 : { LONG(0x4c504952) LONG(0) LONG(0) LONG(0) } > ROMSTUB
  .text : ALIGN(4) {
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
  _loader_end = .;
  _stack_top = ORIGIN(LRAM) + LENGTH(LRAM);
  /DISCARD/ : { *(.eh_frame*) *(.riscv.attributes) }
}
