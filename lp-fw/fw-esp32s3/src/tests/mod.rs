//! Hardware harnesses, selected by `test_*` features (see build.rs's
//! `fw_harness` cfg). Each replaces the boot path's park loop with a runner.

#[cfg(feature = "test_backtrace_oracle")]
pub mod backtrace_oracle;
#[cfg(feature = "test_loopback")]
pub mod loopback;
#[cfg(feature = "test_button")]
pub mod test_button;
/// The FP conformance rig lives in `lp-xt-fp-harness` — the classic ESP32 runs
/// the same corpus, and the rig is a correctness oracle that must not be
/// duplicated. All this chip owes it is its identity.
///
/// The `env!` calls belong here rather than in the harness: `env!` expands in
/// the crate that names it, so a build stamp read inside that crate would
/// describe *its* compilation, not this firmware's.
#[cfg(feature = "test_xt_fp_conformance")]
pub mod xt_fp_conformance {
    pub fn run_all() -> ! {
        lp_xt_fp_harness::run_all(lp_xt_fp_harness::BoardId {
            chip: "esp32s3",
            build_commit: env!("LP_BUILD_COMMIT"),
            build_dirty: env!("LP_BUILD_DIRTY"),
            build_profile: env!("LP_BUILD_PROFILE"),
        })
    }
}
#[cfg(feature = "test_xt_jit_corpus")]
pub mod xt_jit_corpus;

// ── The validation payloads (M6 P08) ────────────────────────────────────────
//
/// What a payload harness calls this chip, in the in-band
/// `[fw-checks-header]` line and anywhere else it names itself.
///
/// It has to equal the sidecar's `chip`, which `lp-emu-validate` refuses to
/// let disagree (`header.rs`, `TranscriptHeader::agrees_with_inband`,
/// "chip: in-band … vs sidecar …"). On this chip all three names agree —
/// ours, espflash's and the cargo feature's are all `esp32s3` — which is
/// exactly what the classic is the counter-example to (`esp32v3` / `esp32` /
/// `esp32`), so the constant exists here for the shape rather than for a
/// disagreement it has to resolve.
#[cfg(feature = "test_shader_compile_incremental")]
pub const CHIP: &str = "esp32s3";

/// The `shader-compile-stress` payload: the stepped compile pipeline on this
/// chip's own JIT, tick by tick, with the heap either side of every slice —
/// and the milestone's one end-to-end exercise of the D-bus/I-bus alias. See
/// the module docs.
///
/// It differs from the five rigs above in what it is FOR: a rig answers a
/// question once and its capture is read by a person, while a payload's
/// capture is a transcript a replay compares field by field against the same
/// payload on another configuration. Hence the `[fw-checks-header]` line it
/// prints first — the in-band half of the provenance the sidecar also
/// carries.
#[cfg(feature = "test_shader_compile_incremental")]
pub mod incremental_shader_compile;
