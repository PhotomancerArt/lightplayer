/* What stays in RAM when `place-switch-tables-in-ram` is off.
 *
 * esp-hal's default puts three families of anonymous constants in `.data`:
 * interrupt-handler tables, every match lookup table in the image, and the
 * `.rodata.cst*` literal pools. On the S3 that was 20.8 KB of a `dram_seg`
 * where `.data`, `.bss` and `.stack` are zero-sum and the measured stack
 * headroom was about 2 KB — 15,054 B of lookup tables (the model's, the
 * compiler's, `BuiltinId::name`'s two 1 KB tables, esp-hal's `Debug` tables
 * for every GPIO signal) and 5,720 B of merged constant pools, almost all of
 * it belonging to code that never runs from an interrupt. Same lever, same
 * shape as the classic's (`fw-esp32v3/rwdata_hook.x`, #521;
 * docs/reports/2026-09-04-classic-ram-budget.md, lever 3).
 *
 * Turning the flag off wholesale would send `lp_ws281x`'s refill tables to
 * flash, which the ISR-in-RAM rule forbids (nothing on the RMT refill path may
 * read flash). So the flag goes off and this hook names what stays.
 *
 * ⚠️ These are INPUT-SECTION globs, not object-file globs. LTO merges the whole
 * image into one codegen unit, so `*crate.o(...)` matches nothing; what
 * survives is the section name, which rustc derives from the mangled symbol —
 * hence the `*<crate>*` shapes below.
 *
 * The `.rodata.cst*` pools are MERGED (one section holds the constants of every
 * crate at a given alignment), so they move as a whole — and on the S3 they
 * can: no RAM-resident function loads an address inside them (see
 * `just iram-flash-literals-esp32s3`, which counts exactly that). The classic
 * keeps its pools because its image has ISR-path code that reads them.
 *
 * Included from esp-hal's `ld/sections/rwdata.x` inside the `.data` output
 * section, which the linker script reaches before `.rodata`: anything matched
 * here wins, and everything else falls through to flash. Found on the linker
 * search path via `build.rs`'s `cargo:rustc-link-search`.
 *
 * Re-verify with `just iram-flash-literals-esp32s3` after any change: it is
 * the check, not this comment.
 */

/* Interrupt dispatch tables. */
*(.rodata.*_esp_hal_internal_handler*)

/* Lookup tables that RAM-resident functions read: the WS281x refill ISR's
 * three `fill_half` tables, and esp-hal's `mapped_to_raw` (interrupt source
 * mapping, on the dispatch path). esp-rtos's are kept as a precaution (0 B
 * today). Everything else in esp-hal — the `Debug` tables for GPIO signals
 * and peripherals — is non-ISR code and goes to flash. */
*(.rodata..Lswitch.table.*lp_ws281x*)
*(.rodata..Lswitch.table.*mapped_to_raw*)
*(.rodata..Lswitch.table.*esp_rtos*)

/* Jump tables of `#[ram]` FUNCTIONS are emitted as `.rodata.<function>` (not
 * `.rodata..Lswitch.table.*`) and fall through to flash — the classic's debt
 * entry docs/debt/classic-iram-handlers-reach-flash.md names the two that sit
 * on an executed path there (`__level_*_interrupt`, `__pender`). They are not
 * touched by this hook either way: the flag only concerns `Lswitch` tables and
 * the pools. */
