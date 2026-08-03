ENTRY(_entry)

/* Emulator provides 576KB RAM */
STACK_SIZE = 0x30000;  /* 192KB stack */

/* Guest heap size.
 *
 * ⚠️ This is deliberately NOT the device's arena, and setting it to one would
 * make the emulator LESS faithful, not more. Measured 2026-08-02: the guest
 * carries ~52 KB of harness baseline the firmware does not — live bytes at
 * project-load start were 63,596 B here versus ~10,936 B idle on a classic
 * ESP32. Sizing this to the classic's 178,176 B arena would make the emulator
 * OOM on projects the device runs comfortably.
 *
 * What DOES transfer between emulator and silicon is the *deltas*: emulator
 * project-load cost measured 51,723 B against 53,052 B on the classic for the
 * same project — within 2.6%. So derive memory budgets from window deltas
 * (project-load, shader-compile) rather than from whether the guest survives
 * at a device-sized heap. See the plan directory referenced in
 * docs/adr/2026-08-01-esp32v3-flash-budget.md.
 *
 * Override for pressure experiments (verified 2026-08-02; note there is no
 * `-Wl,` prefix — rust-lld is invoked directly, not through a compiler driver,
 * and rejects it):
 *   RUSTFLAGS="-C link-arg=--defsym=__lp_heap_size=<bytes>"
 */
HEAP_SIZE = DEFINED(__lp_heap_size) ? __lp_heap_size : 320K;

MEMORY {
    ROM (rx)  : ORIGIN = 0x00000000, LENGTH = 0x80000000 /* Set to maximum possible length */
    RAM (wrx) : ORIGIN = 0x80000000, LENGTH = 0x00090000 /* 576KB RAM */
}

SECTIONS {
    /* Unwinding (fde-static) needs these symbols */
    PROVIDE(__executable_start = ORIGIN(ROM));

    .text : ALIGN(4) {
        KEEP (*(.text.init.entry))
        KEEP (*(.text.init.*))
        KEEP (*(SORT_NONE(.init)))
        *(.text*);
        . = ALIGN(8);
    } > ROM

    .rodata : ALIGN(4) {
        *(.rodata)
        *(.rodata*);
        *(.gcc_except_table*);
        . = ALIGN(8);
    } > ROM

    .preinit_array : ALIGN(4) {
        __preinit_array_start = .;
        KEEP(*(.preinit_array*));
        __preinit_array_end = .;
    } > ROM

    .init_array : ALIGN(4) {
        __init_array_start = .;
        KEEP(*(SORT_BY_INIT_PRIORITY(.init_array.*)));
        KEEP(*(.init_array*));
        __init_array_end = .;
    } > ROM

    .fini_array : ALIGN(4) {
        __fini_array_start = .;
        KEEP(*(SORT_BY_INIT_PRIORITY(.fini_array.*)));
        KEEP(*(.fini_array*));
        __fini_array_end = .;
    } > ROM

    . = ALIGN(8);
    PROVIDE(__etext = .);

    /* This guest is abort tier and never unwinds (ADR
       2026-08-02-rv32-firmwares-are-abort-tier), so unwind tables are dead
       weight in ROM. Discarded explicitly rather than left to orphan-section
       placement, matching lp-shader/lps-builtins-emu-app/memory.ld. */
    /DISCARD/ : {
        *(.eh_frame .eh_frame.*)
        *(.gcc_except_table .gcc_except_table.*)
    }

    .bss (NOLOAD) : ALIGN(4) {
        __bss_target_start = .;
        _bss = .;
        *(.bss*);
        *(.sbss*);
        *(.sbss2*);
        *(COMMON*);
        . = ALIGN(8);
        __bss_target_end = .;
    } > RAM

    .data : ALIGN(4) {
        __data_target_start = .;
        *(.data .data.*)
        . = ALIGN(8);
        __global_pointer$ = . + 0x800;
        *(.sdata .sdata.*)
        *(.sdata2*);
        . = ALIGN(8);
        *(.srodata.cst16)
    	*(.srodata.cst8)
    	*(.srodata.cst4)
    	*(.srodata.cst2)
    	*(.srodata .srodata.*)
        . = ALIGN(8);
        __data_target_end = .;
        _end = .;
    } > RAM AT > ROM
    __data_source_start = LOADADDR(.data);

    /* Persistent scratch (crash-recovery region): NOT zeroed or copied by
       the entry code, so host-written contents survive guest startup —
       the emulator analog of RTC fast RAM. */
    .noinit (NOLOAD) : ALIGN(8) {
        *(.noinit .noinit.*);
        . = ALIGN(8);
    } > RAM

    /* Heap section - reserve space for dynamic allocation */
    /* NOLOAD means this section won't be loaded from the ELF, but space is reserved */
    .heap (NOLOAD) : ALIGN(16) {
        __heap_start = .;
        . += HEAP_SIZE; /* see HEAP_SIZE above — NOT a device-accurate arena */
        . = ALIGN(16);
        __heap_end = .;
    } > RAM
    
    /* Stack section - comes AFTER heap, grows downward */
    /* Stack pointer will be set to __stack_start (top of stack) */
    .stack (NOLOAD) : ALIGN(16) {
        . += STACK_SIZE;
        . = ALIGN(16);
        __stack_start = . ;
    } > RAM

    _ram  = ORIGIN(RAM);
    _eram = ORIGIN(RAM) + LENGTH(RAM);
}

