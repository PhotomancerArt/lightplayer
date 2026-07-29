/* Linker script for the Xtensa builtins image.
 *
 * Same address model as fixtures/link.ld (see its header for the full
 * rationale) — the emulator's 128 KiB code region at D-bus
 * 0x3FC88000..0x3FCA8000, I-bus alias +0x6F0000 — but with a **different
 * split of that region**.
 *
 * The fixtures script splits it evenly (64 KiB text / 64 KiB data), which
 * matches lp-xt-elf's `DATA_BASE = CODE_DBUS_BASE + CODE_REGION_LEN/2`
 * convention its reloc engine relies on. This image is the opposite shape:
 * it is nearly all code (every `__lps_*` builtin — ~71 KiB, which overflows
 * a 64 KiB text region) and carries almost no data. So it takes 112 KiB of
 * text and 16 KiB of data, staying inside the same 128 KiB region.
 *
 * Nothing hardcodes this split: `rt_emu_xt` loads PT_LOAD segments at their
 * p_vaddr straight out of this ELF, so the addresses come from the image
 * itself. The fixtures' even split and the reloc engine's DATA_BASE are
 * untouched.
 */

ENTRY(_start)

MEMORY
{
  IRAM (x)  : ORIGIN = 0x40378000, LENGTH = 112K   /* code region bytes 0..112K   */
  DRAM (rw) : ORIGIN = 0x3FCA4000, LENGTH = 16K    /* code region bytes 112K..128K */
}

SECTIONS
{
  /* l32r is backward-only, so literal pools must precede the text using them. */
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
