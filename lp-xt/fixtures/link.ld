/* Linker script for lp-xt-emu guest fixtures.
 *
 * Address model (must match lp-xt-emu's modeled memory, lp-xt-emu/src/memory.rs
 * and emu.rs):
 *   - The emulator maps SRAM1 at D-bus 0x3FC88000..0x3FCA8000 (128 KiB, the
 *     "code region") with the hardware I-bus alias at +0x6F0000, and a separate
 *     stack region at D-bus 0x3FCC0000..0x3FCE0000 (the emulator sets SP; the
 *     guest never touches the linker for its stack).
 *   - Code must execute at I-bus addresses, so .text (with its l32r literal
 *     pools) is linked at 0x40378000 = ibus_alias(0x3FC88000), backing bytes
 *     0..64K of the code region.
 *   - Data lives at plain D-bus addresses in the second half of the same
 *     region, 0x3FC98000 (backing bytes 64K..128K) — no overlap with text.
 *
 * The ELF loader (lp-xt-elf) copies PT_LOAD segments to their p_vaddr; both
 * views resolve to the same backing store, exactly as on the S3.
 */

ENTRY(_start)

MEMORY
{
  IRAM (x)  : ORIGIN = 0x40378000, LENGTH = 64K
  DRAM (rw) : ORIGIN = 0x3FC98000, LENGTH = 64K
}

SECTIONS
{
  /* Literals are emitted with the code (LLVM places pools before their
   * function); keep the esp-idf-style spec so any .literal input sections
   * also land ahead of the text that l32r's them (l32r is backward-only). */
  .text : ALIGN(4)
  {
    *(.literal .literal.*)
    *(.text .text.*)
  } > IRAM

  .rodata : ALIGN(4)
  {
    *(.rodata .rodata.*)
    *(.srodata .srodata.*)
  } > DRAM

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
