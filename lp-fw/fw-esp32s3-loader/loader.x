/* The ESP32-S3 split-image loader lives entirely in internal SRAM1: it
   remaps the flash MMU under the core's link addresses, so it must not run
   from flash.

   Code runs from the instruction-bus view, data from the data-bus view of
   the same SRAM (I 0x403A_0000 = D 0x3FCB_0000, the +0x6F_0000 alias):
   lp_bootctl::S3_LOADER_RAM, 0x3FCB_0000..0x3FCC_0000, written into
   loader_memory.x by build.rs. That range lies inside every core's .bss
   (never inside a segment the loader copies; esp_image refuses one that
   reaches it) and below the IDF bootloader's own RAM
   (lp_bootctl::S3_IDF_BOOTLOADER_RAM, 0x3FCD_9700 and up, D view), which is
   still live while this image runs: the loader runs on the stack the IDF
   bootloader called it on. The core zeroes .bss over the loader after the
   jump. */
INCLUDE loader_memory.x

MEMORY {
  /* The IDF bootloader (espflash 3.3's bundled S3 build) maps an app's
     DROM and IROM segments and asserts the map is page-aligned; with none
     it asserts on zeroes it computed (`abort()` at 0x403ceab9). So, as the
     C6 loader does, the loader carries two 16-byte placeholders in ONE
     flash page: the data view's at 0x3C00_0020 and the instruction view's at
     0x4200_0120, both MMU entry 0. The loader never reads them; it remaps
     entry 0 for the core. The data view's carries the loader's version word
     ("LPLV", version, 0 — lp_bootctl::loader_identity), which a core and
     lp-fw-split find by scanning the loader image's first 4 KiB of flash. */
  DSTUB : ORIGIN = 0x3C000020, LENGTH = 0x100
  ISTUB : ORIGIN = 0x42000120, LENGTH = 0x100
}
ENTRY(_start)

/* The S3 mask ROM's jump table (esp32s3_rev0_rom.elf's `__call_*`). */
PROVIDE(ets_printf = 0x400005d0);
PROVIDE(rtc_get_reset_reason = 0x4000057c);
PROVIDE(Cache_Invalidate_ICache_All = 0x400016d4);
PROVIDE(Cache_Invalidate_DCache_All = 0x400016e0);

SECTIONS {
  .drom_stub : { KEEP(*(.loader_identity)) LONG(0) LONG(0) } > DSTUB
  .irom_stub : { LONG(0x52494c50) LONG(0) LONG(0) LONG(0) } > ISTUB
  .text : ALIGN(4) {
    KEEP(*(.literal._start .text._start))
    *(.literal .text .literal.* .text.*)
    . = ALIGN(4);
  } > LIRAM
  /* The IDF bootloader loads file bytes only: .bss rides in .data as
     zeroes so nothing reads stale RAM. */
  .data : ALIGN(4) {
    *(.rodata .rodata.*)
    *(.data .data.*)
    *(.bss .bss.* COMMON)
    . = ALIGN(4);
  } > LDRAM
  /DISCARD/ : { *(.eh_frame*) *(.xtensa.info) *(.comment) }
}
