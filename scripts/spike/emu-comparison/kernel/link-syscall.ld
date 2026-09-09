/* SPIKE — plain static ELF for user-mode emulators (libriscv, rv32emu). */
ENTRY(_start)
MEMORY { RAM (rwx) : ORIGIN = 0x00010000, LENGTH = 8M }
SECTIONS {
  .text : { KEEP(*(.text.init)) *(.text .text.*) } > RAM
  .rodata : { *(.rodata .rodata.*) } > RAM
  .data : { *(.data .data.*) *(.sdata .sdata.*) } > RAM
  .bss : { *(.bss .bss.*) *(.sbss .sbss.*) *(COMMON) } > RAM
  . = ALIGN(16);
  _stack_top = ORIGIN(RAM) + 1M;
  /DISCARD/ : { *(.eh_frame*) *(.comment) }
}
