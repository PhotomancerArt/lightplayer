//! Benchmark images: the shipped firmware with something added, never
//! replaced.
//!
//! Distinct from `src/tests/`, which holds `test_*` harnesses that take over
//! `main` (`build.rs` turns any `CARGO_FEATURE_TEST_*` into
//! `cfg(fw_harness)`). Everything here runs *inside* the product boot, so a
//! measurement taken here is a measurement of the product. `frame-dump` is
//! the precedent already in this crate's `Cargo.toml`: a feature that
//! decorates the app path instead of replacing it.
pub mod render_loop;
