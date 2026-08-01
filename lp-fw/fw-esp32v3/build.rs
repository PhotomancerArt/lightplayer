//! Build script for fw-esp32v3.
//!
//! Deliberately minimal, mirroring fw-esp32s3's: this chip is also abort-tier
//! (ADR 2026-07-29-per-chip-fw-toolchains), so there is no `.eh_frame`
//! patching to do — that machinery is the C6's `panic=unwind` tier only.
//!
//! Unlike fw-esp32s3's build.rs, this one does not yet emit the
//! `fw_harness` cfg or the `LP_BUILD_*` provenance env vars: P1 has no
//! `test_*` harness features and no wire-hello to stamp (no `lpc-wire`
//! dependency). Both are straightforward to port from fw-esp32s3's build.rs
//! when a later phase adds a harness feature or the wire-protocol server.

use std::path::PathBuf;

fn main() {
    // Watch the whole package, not just this file — same reasoning as
    // fw-esp32s3's build.rs: emitting any rerun-if-changed disables cargo's
    // default "re-run on any package file change" rule, so naming only
    // `build.rs` would pin build outputs to a stale checkout state.
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).display()
    );
}
