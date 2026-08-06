//! Build script for `lp-xt-fp-harness`.
//!
//! It exists for exactly one reason: the harness reads its mode, family filter
//! and vector limit through `option_env!`, and **cargo does not track those on
//! its own**. Without these lines, changing `LP_FP_FAMILY` reuses the previous
//! build and the board silently runs the wrong subset — a stale binary that
//! looks like a successful run.
//!
//! This moved here with the harness. It used to live in `fw-esp32s3/build.rs`,
//! which was correct while the `option_env!` calls were in that crate; they are
//! in this one now, and the tracking has to follow the macro, not the feature.
//! The firmware build scripts keep their own copies for the `fw_harness` cfg,
//! which is a different job.
//!
//! Named explicitly rather than wildcarded because cargo has no wildcard here.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for var in ["LP_FP_MODE", "LP_FP_FAMILY", "LP_FP_LIMIT"] {
        println!("cargo:rerun-if-env-changed={var}");
    }
}
