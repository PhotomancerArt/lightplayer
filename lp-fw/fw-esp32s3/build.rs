//! Build script for fw-esp32s3.
//!
//! Deliberately minimal. The C6 counterpart patches esp-hal's `eh_frame.x` so
//! `.eh_frame` survives into ROM for `unwinding`-based panic recovery; this
//! chip uses **abort-tier** recovery (ADR 2026-07-29-per-chip-fw-toolchains),
//! so there are no unwind tables to preserve and none of that machinery
//! belongs here.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Harness builds: any `test_*` feature selects a hardware harness
    // entrypoint instead of the app. Collapsed to a single cfg so app-only
    // code carries one gate rather than a wall of per-feature conditions —
    // the same shape fw-esp32c6 uses, deliberately not a second mechanism.
    println!("cargo::rustc-check-cfg=cfg(fw_harness)");
    let harness = std::env::vars().any(|(k, _)| k.starts_with("CARGO_FEATURE_TEST_"));
    if harness {
        println!("cargo::rustc-cfg=fw_harness");
    }
}
