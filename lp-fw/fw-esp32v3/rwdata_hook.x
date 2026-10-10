/* What stays in RAM when `place-switch-tables-in-ram` is off.
 *
 * esp-hal's default puts three families of anonymous constants in `.data`:
 * interrupt-handler tables, every match jump table in the image, and the
 * `.rodata.cst*` literal pools. On the classic that was 17,896 B out of a
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
 * crate at a given alignment, so they can only move as a whole. They stayed in
 * RAM (5,080 B) on the 2026-09-06 build because some of the code that reads
 * them was believed to be on the ISR path. 2026-10-10 (E5 of the RAM research
 * program): `just iram-flash-literals-esp32v3` — the check — says no
 * RAM-resident function loads an address inside them (99 functions, 82 flash
 * literals, both before and after), so the pools went to flash too and the
 * stack got 5,520 B. If a RAM function ever does read one, the check goes red
 * naming it; put `*(.rodata.cst*)` back below and say why.
 *
 * Included from esp-hal's `ld/sections/rwdata.x` inside the `.data` output
 * section, which the linker script reaches before `.rodata` — so anything
 * matched here wins, and everything else falls through to flash. Found on the
 * linker search path via `build.rs`'s `cargo:rustc-link-search`.
 *
 * Re-verify with `just iram-flash-literals-esp32v3` after any change: it is the
 * check, not this comment.
 */

/* Interrupt dispatch tables. (The merged constant pools used to be here too.) */
*(.rodata.*_esp_hal_internal_handler*)
*(.rodata.*INTERRUPT_EDGE*)

/* Lookup tables that RAM-resident functions read: the WS281x refill ISR's
 * `fill_half` tables and esp-hal's `mapped_to_raw` (interrupt source mapping),
 * with esp-rtos's as a precaution (0 B). This used to be five broad crate
 * globs (`*fw_esp32v3*`, `*esp_hal*`, `*xtensa_lx_rt*`, …) that also kept
 * `boot_firmware`'s 1 KB table and the `Debug` tables of every GPIO signal and
 * exception cause in RAM (2,448 B); `just iram-flash-literals-esp32v3` says no
 * RAM-resident function loads those either (99 functions / 82 literals, both
 * ways), so only the ones named here stay. */
*(.rodata..Lswitch.table.*lp_ws281x*)
*(.rodata..Lswitch.table.*mapped_to_raw*)
*(.rodata..Lswitch.table.*esp_rtos*)

/* Jump tables of `#[ram]` FUNCTIONS, which LLVM emits as `.rodata.<function>`
 * (not `.rodata..Lswitch.table.*`, which is only the switch LOOKUP tables):
 * esp-hal's level-3 dispatch matches on the CPU-internal source number, and
 * esp-rtos's embassy `__pender` matches on the executor id. Both run on every
 * peripheral interrupt / wake, both used to fall through to flash `.rodata`
 * (docs/debt/classic-iram-handlers-reach-flash.md). ~0xd0 B together. */
*(.rodata.__level_*_interrupt)
*(.rodata.__pender)
