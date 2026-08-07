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
