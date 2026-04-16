ENTRY(_entry)

/* Emulator provides 512KB RAM - balance for Cranelift */
STACK_SIZE = 0x30000;  /* 192KB stack */

MEMORY {
    ROM (rx)  : ORIGIN = 0x00000000, LENGTH = 0x80000000 /* Set to maximum possible length */
    RAM (wrx) : ORIGIN = 0x80000000, LENGTH = 0x00080000 /* 512KB RAM to match emulator */
}

SECTIONS {
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

    /* Heap section - reserve space for dynamic allocation */
    /* NOLOAD means this section won't be loaded from the ELF, but space is reserved */
    .heap (NOLOAD) : ALIGN(16) {
        __heap_start = .;
        . += 256 * 1024; /* 256KB heap */
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

    /DISCARD/ : {
        *(.eh_frame .eh_frame.*)
    }
}

