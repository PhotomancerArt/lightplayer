/* What stays in RAM when `place-switch-tables-in-ram` is off.
 *
 * esp-hal's default puts three families of anonymous constants in `.data`:
 * interrupt-handler tables, every match jump table in the image, and the
 * `.rodata.cst*` literal pools. On the classic that is 17,896 B out of a
 * `dram_seg` where `.data`, `.bss` and `.stack` are strictly zero-sum, and
 * almost all of it belongs to code that never runs from an ISR.
 *
 * Turning the flag off wholesale is not safe here: it sends `lp_ws281x`'s
 * refill jump tables and esp-hal's `INTERRUPT_EDGE` table to flash, and the
 * ISR-in-RAM rule (memory note `isr-path-in-ram-rule`) says nothing on the RMT
 * refill path may read flash. So the flag goes off and this hook names the
 * exceptions.
 *
 * ⚠️ These are INPUT-SECTION globs, not object-file globs. LTO merges the whole
 * image into one codegen unit, so `*crate.o(...)` matches nothing; what does
 * survive is the section name, which rustc derives from the mangled symbol —
 * hence the `*<crate>*` shapes below.
 *
 * ⚠️ `.rodata.cst*` are MERGED pools: one section holds constants from every
 * crate at a given alignment, so they can only move as a whole. They stay in
 * RAM (5,080 B) because some of the code that reads them is on the ISR path
 * and there is no way to split them.
 *
 * Included from esp-hal's `ld/sections/rwdata.x` inside the `.data` output
 * section, which the linker script reaches before `.rodata` — so anything
 * matched here wins, and everything else falls through to flash. Found on the
 * linker search path via `build.rs`'s `cargo:rustc-link-search`.
 *
 * Re-verify with `just iram-flash-literals-esp32v3` after any change: it is the
 * check, not this comment.
 */

/* Interrupt dispatch tables and the merged constant pools. */
*(.rodata.*_esp_hal_internal_handler*)
*(.rodata.*INTERRUPT_EDGE*)
*(.rodata.cst*)

/* Jump tables belonging to crates with code in IRAM: the WS281x refill ISR,
 * the firmware's own ISR plumbing and wire pusher, esp-hal's and esp-rtos's
 * handlers, and xtensa-lx-rt's vector code. */
*(.rodata..Lswitch.table.*lp_ws281x*)
*(.rodata..Lswitch.table.*fw_esp32v3*)
*(.rodata..Lswitch.table.*esp_hal*)
*(.rodata..Lswitch.table.*esp_rtos*)
*(.rodata..Lswitch.table.*xtensa_lx_rt*)

/* Jump tables of `#[ram]` FUNCTIONS, which LLVM emits as `.rodata.<function>`
 * (not `.rodata..Lswitch.table.*`, which is only the switch LOOKUP tables):
 * esp-hal's level-3 dispatch matches on the CPU-internal source number, and
 * esp-rtos's embassy `__pender` matches on the executor id. Both run on every
 * peripheral interrupt / wake, both used to fall through to flash `.rodata`
 * (docs/debt/classic-iram-handlers-reach-flash.md). ~0xd0 B together. */
*(.rodata.__level_*_interrupt)
*(.rodata.__pender)
