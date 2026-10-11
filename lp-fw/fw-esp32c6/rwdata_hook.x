/* What stays in RAM when `place-switch-tables-in-ram` is off.
 *
 * esp-hal's default puts three families of anonymous constants in `.data`:
 * interrupt-handler tables, every match lookup table in the image, and the
 * `.rodata.cst*` literal pools. On the C6 that was 10.4 KB of a 512 KiB SRAM
 * where `.data`, `.bss` and `.stack` are zero-sum (the main stack is whatever
 * the statics leave), and almost all of it belongs to code that never runs
 * from an interrupt: `lps_builtin_ids::BuiltinId::name` alone is two 1 KB
 * tables. Same lever, same shape as the classic's (`fw-esp32v3/rwdata_hook.x`,
 * docs/reports/2026-09-04-classic-ram-budget.md, lever 3).
 *
 * Turning the flag off wholesale would send `lp_ws281x`'s refill tables to
 * flash, which the ISR-in-RAM rule forbids (nothing on the RMT refill path
 * may read flash). So the flag goes off and this hook names what stays.
 *
 * ⚠️ These are INPUT-SECTION globs, not object-file globs. LTO merges the whole
 * image into one codegen unit, so `*crate.o(...)` matches nothing; what
 * survives is the section name, which rustc derives from the mangled symbol —
 * hence the `*<crate>*` shapes below.
 *
 * ⚠️ `.rodata.cst*` are MERGED pools: one section holds constants from every
 * object at a given alignment, so they can only move as a whole, and they stay
 * in RAM (4.7 KB) because RAM-resident code reads entries of them: the blobs'
 * `pm_beacon_offset_get_average` / `_get_expect`, esp-radio's `semphr_give` /
 * `semphr_take`, esp-hal's default GPIO interrupt handler (`.LCPI…`). The
 * blobs themselves contribute only 84 B of the 4,752 B (their archives' own
 * `.rodata.cst*`, measured with E5's `ram-e05-blob-cst.py`); the rest is
 * Rust's, one LTO object, which cannot be split by file.
 *
 * Included from esp-hal's `ld/sections/rwdata.x` inside the `.data` output
 * section, which the linker script reaches before `.rodata`: anything matched
 * here wins, and everything else falls through to flash. Found on the linker
 * search path via `build.rs`'s `cargo:rustc-link-search`.
 *
 * Re-verify with `just iram-data-refs-esp32c6` after any change: it is the
 * check (every constant a RAM-resident function reads must be in RAM or be
 * one of the already-accepted cold-path flash reads), not this comment.
 */

/* Interrupt dispatch tables and the merged constant pools. */
*(.rodata.*_esp_hal_internal_handler*)
*(.rodata.cst*)

/* Lookup tables belonging to crates with code in IRAM: the WS281x refill ISR
 * (its three `fill_half` tables are the only ones a RAM function reads in
 * this image), and esp-hal's and esp-rtos's interrupt plumbing — kept as a
 * precaution, 0 B today. */
*(.rodata..Lswitch.table.*lp_ws281x*)
*(.rodata..Lswitch.table.*esp_hal*)
*(.rodata..Lswitch.table.*esp_rtos*)
