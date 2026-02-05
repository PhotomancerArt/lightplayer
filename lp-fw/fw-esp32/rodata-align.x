/* Linker script fragment to force 8-byte alignment for .rodata sections
 * 
 * This prevents the ESP32 bootloader from splitting .rodata_desc and .rodata
 * into separate MAP segments. The bootloader expects at most 2 MAP segments
 * (DROM/IROM), but with 4-byte alignment, the conversion tool creates 3 segments.
 * 
 * By forcing 8-byte alignment for both .rodata_desc and .rodata, they merge
 * into a single MAP segment, satisfying the bootloader's requirement.
 * 
 * This fragment is included AFTER esp-rs's default linker script (linkall.x),
 * so it overrides the section definitions. We don't specify memory regions
 * here to avoid conflicts - the linker will use the memory regions already
 * defined by esp-rs's linker script.
 */

SECTIONS {
    /* Force 8-byte alignment for .rodata_desc (created by esp_app_desc! macro) */
    .rodata_desc : ALIGN(8) {
        KEEP(*(.rodata_desc .rodata_desc.*))
    }

    /* Force 8-byte alignment for .rodata section */
    .rodata : ALIGN(8) {
        *(.rodata)
        *(.rodata.*)
        *(.gnu.linkonce.r.*)
        . = ALIGN(8);
    }
}
