//! Benchmark images: the shipped firmware with something added, never
//! replaced.
//!
//! Distinct from `src/tests/`, which holds `test_*` harnesses that take over
//! `main` (`build.rs` turns any `CARGO_FEATURE_TEST_*` into
//! `cfg(fw_harness)`). Everything here runs *inside* the product boot, so a
//! measurement taken here is a measurement of the product.
pub mod render_loop;
