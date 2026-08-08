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
