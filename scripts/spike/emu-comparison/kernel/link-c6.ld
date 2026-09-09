/* SPIKE — ESP32-C6 HP SRAM, the region lp-emu-esp32c6 direct-loads into. */
ENTRY(_start)
MEMORY { RAM (rwx) : ORIGIN = 0x40800000, LENGTH = 384K }
SECTIONS {
  .text : { KEEP(*(.text.init)) *(.text .text.*) } > RAM
  .rodata : { *(.rodata .rodata.*) } > RAM
  .data : { *(.data .data.*) *(.sdata .sdata.*) } > RAM
  .bss : { *(.bss .bss.*) *(.sbss .sbss.*) *(COMMON) } > RAM
  . = ALIGN(16);
  _stack_top = ORIGIN(RAM) + LENGTH(RAM);
  /DISCARD/ : { *(.eh_frame*) *(.comment) }
}
