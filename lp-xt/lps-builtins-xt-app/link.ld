/* Linker script for the Xtensa builtins image.
 *
 * This image is **firmware**, and firmware on an ESP32 executes from flash
 * through the cache (XIP). So it links exactly where a real app's flash
 * segments go, and nowhere near the SRAM the JIT writes into:
 *
 *   .text            -> IROM, the flash instruction window (read-only, XIP)
 *   .rodata          -> DROM, the flash data window (read-only)
 *   .data/.bss       -> DRAM, internal SRAM
 *
 * The addresses are lp-xt-emu's BoardProfile::esp32s3() — see board.rs for the
 * provenance of each (esp-hal's MIT/Apache-2.0 ld/esp32s3/memory.x, plus
 * in-repo corroboration). Lengths here are the *modeled* window lengths, not
 * the hardware ones (hardware gives 32 MB of each flash window); a segment
 * that outgrew them would fail to link here rather than fail to load later.
 *
 * Contrast fixtures/link.ld, which links its guests into the SRAM code region:
 * those are raw payloads a runner writes into RAM, which is a different thing
 * from a flash-resident image and stays as it is.
 *
 * No boot-time copy machinery, and LMA = VMA throughout: the emulator's loader
 * writes each PT_LOAD segment straight to its p_vaddr, the way a flasher plus a
 * configured cache MMU would leave memory. There is no second-stage bootloader
 * here to copy .data out of flash, which is why .data/.bss get an SRAM VMA
 * directly.
 *
 * Nothing downstream hardcodes this split: `rt_emu::xt_image` classifies each
 * segment by its p_vaddr against the board profile, so moving a section here
 * moves it there.
 */

ENTRY(_start)

MEMORY
{
  /* Flash, mapped through the cache. Read-only in the emulator's model, as on
   * hardware. 256K/64K are BoardProfile::MODELED_IROM_LEN / MODELED_DROM_LEN. */
  IROM (rx) : ORIGIN = 0x42000000, LENGTH = 256K
  DROM (r)  : ORIGIN = 0x3C000000, LENGTH = 64K

  /* Internal SRAM between the emulator's code region (ends 0x3FCA8000) and its
   * stack region (starts 0x3FCC0000) — BoardProfile::image_data_base. */
  DRAM (rw) : ORIGIN = 0x3FCA8000, LENGTH = 32K
}

SECTIONS
{
  /* l32r is backward-only, so literal pools must precede the text using them.
   * Both live in IROM: a literal is loaded from the instruction stream. */
  .text : ALIGN(4)
  {
    *(.literal .literal.*)
    *(.text .text.*)
  } > IROM

  .rodata : ALIGN(4)
  {
    *(.rodata .rodata.*)
    *(.srodata .srodata.*)
  } > DROM

  .data : ALIGN(4)
  {
    *(.data .data.*)
    *(.sdata .sdata.*)
  } > DRAM

  .bss (NOLOAD) : ALIGN(4)
  {
    *(.bss .bss.*)
    *(.sbss .sbss.*)
    *(COMMON)
  } > DRAM

  /DISCARD/ :
  {
    *(.eh_frame)
    *(.eh_frame_hdr)
    *(.comment)
    *(.xtensa.info)
    *(.xt.prop .xt.prop.*)
    *(.xt.lit .xt.lit.*)
  }
}
