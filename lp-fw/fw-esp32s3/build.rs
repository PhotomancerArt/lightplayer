//! Build script for fw-esp32s3.
//!
//! Deliberately minimal. The C6 counterpart patches esp-hal's `eh_frame.x` so
//! `.eh_frame` survives into ROM for `unwinding`-based panic recovery; this
//! chip uses **abort-tier** recovery (ADR 2026-07-29-per-chip-fw-toolchains),
//! so there are no unwind tables to preserve and none of that machinery
//! belongs here.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
}
