//! Hardware harnesses, selected by `test_*` features (see build.rs's
//! `fw_harness` cfg). Each replaces the boot path's app with a runner.

/// The FP conformance rig lives in `lp-xt-fp-harness` — fw-esp32s3 runs the
/// same corpus on its LX7, and the rig is a correctness oracle that must not be
/// duplicated. All this chip owes it is its identity.
///
/// The `env!` calls belong here rather than in the harness: `env!` expands in
/// the crate that names it, so a build stamp read inside that crate would
/// describe *its* compilation, not this firmware's.
#[cfg(feature = "test_xt_fp_conformance")]
pub mod xt_fp_conformance {
    pub fn run_all() -> ! {
        lp_xt_fp_harness::run_all(lp_xt_fp_harness::BoardId {
            chip: "esp32",
            build_commit: env!("LP_BUILD_COMMIT"),
            build_dirty: env!("LP_BUILD_DIRTY"),
            build_profile: env!("LP_BUILD_PROFILE"),
        })
    }
}

/// Silicon rig for the interrupt-executor wake assumptions (ADR
/// 2026-08-25-classic-uart-io-task-executor-isolation) — and the esp-rtos
/// upgrade canary. See the module docs.
#[cfg(feature = "test_interrupt_executor")]
pub mod interrupt_executor;

/// Silicon probe for the JIT code region's SRAM0 placement: word-only
/// access, execute-from-SRAM0, barrier need, `.rwtext` end. See the module
/// docs.
#[cfg(feature = "test_sram0_exec")]
pub mod sram0_exec;

/// Silicon discriminator for the APP core's ROM reset path: does starting
/// core 1 re-run the mask ROM's unpack/bss tables over heap region 0? See the
/// module docs.
#[cfg(feature = "test_appcore_rom_path")]
pub mod appcore_rom_path;

// ── The validation payloads (M5 P2) ─────────────────────────────────────────
//
/// What a payload harness calls this chip, in the in-band
/// `[fw-checks-header]` line and anywhere else it names itself.
///
/// `esp32v3`, not `esp32`. It has to equal the sidecar's `chip`, which
/// `lp-emu-validate` refuses to let disagree (`header.rs`,
/// `TranscriptHeader::agrees_with_inband`, "chip: in-band … vs sidecar …"),
/// and the validation system's name for this chip is `esp32v3`. `esp32` is
/// the espflash chip name and this crate's cargo feature — three names for
/// one part, and M5 notes.md §1.1 is the table that keeps them apart.
#[cfg(any(
    feature = "test_gpio_calibrate",
    feature = "test_cycle_probe",
    feature = "test_shader_compile_incremental"
))]
pub const CHIP: &str = "esp32v3";

//
// Three payload harnesses, each the CHIP HALF of a `fw-checks` payload: the
// portable logic is in that crate and this crate supplies the chip — the
// link, the pads, the clocks, the assembly. `fw-checks` is MIRRORED, never a
// `fw-esp32c6` module imported, which is what keeps "the shared payload logic
// is shared and the chip half is chip code" true rather than aspirational.
//
// They differ from the three rigs above in what they are FOR: a rig answers a
// question once and its capture is read by a person, while a payload's
// capture is a transcript a replay compares field by field against the same
// payload on another configuration. Hence the `[fw-checks-header]` line each
// prints first — the in-band half of the provenance the sidecar also carries.

/// The `gpio-calibrate` payload: the host drives one pad at a time and the
/// device reports a ramping square wave. See the module docs — in particular
/// the pad policy, which is this chip's and not the C6's.
#[cfg(feature = "test_gpio_calibrate")]
pub mod gpio_calibrate;

/// The `cycle-probe` payload: one kernel per term of a cycle model, bracketed
/// by CCOUNT and by TIMG0's LACT microseconds. **Recorded, never gated.**
#[cfg(feature = "test_cycle_probe")]
pub mod cycle_probe;

/// The `shader-compile-stress` payload: the stepped compile pipeline on this
/// chip's own JIT, tick by tick, with the heap either side of every slice.
#[cfg(feature = "test_shader_compile_incremental")]
pub mod shader_compile_incremental;
