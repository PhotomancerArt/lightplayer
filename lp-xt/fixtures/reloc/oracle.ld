/* GNU-ld oracle script for the reloc fixtures: the same address model the
 * lp-xt-elf `reloc` prototype uses (and fixtures/link.ld documents) — text +
 * literals at the I-bus alias 0x40378000, data/bss at D-bus 0x3FC98000. The
 * resulting executables are behavioral oracles only: lp-xt-elf's own linker is
 * NOT expected to be byte-identical (GNU ld relaxes; we don't), just to run to
 * the same result.
 */
ENTRY(lp_main)

MEMORY
{
  IRAM (x)  : ORIGIN = 0x40378000, LENGTH = 64K
  DRAM (rw) : ORIGIN = 0x3FC98000, LENGTH = 64K
}

SECTIONS
{
  .text : ALIGN(4)
  {
    *(.literal .literal.*)
    *(.text .text.*)
  } > IRAM

  .data : ALIGN(4)
  {
    *(.rodata .rodata.*)
    *(.data .data.*)
  } > DRAM

  .bss : ALIGN(4)
  {
    *(.bss .bss.*)
    *(COMMON)
  } > DRAM

  /DISCARD/ :
  {
    *(.comment)
    *(.xtensa.info)
    *(.xt.prop .xt.prop.*)
    *(.xt.lit .xt.lit.*)
  }
}
