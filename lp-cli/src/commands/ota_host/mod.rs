//! The host side of an over-the-air update, as `lp-cli` drives it: the
//! flags (`--ota-offer`, `--ota-cache`, …) shared by `emu run --host-link`
//! and `link capture`, the offered build read from a package's
//! `ota-manifest.json`, and [`OtaHost`], the edge around `lpa-update`'s
//! `UpdateDriver` — it does the IO (the engine cache directory) and the
//! clock, and resolves the driver's effects. No update logic lives here: it
//! is `lpa-update`'s.

pub mod ota_args;
pub mod ota_host;
pub mod ota_offer_dir;

pub use ota_args::OtaArgs;
pub use ota_host::OtaHost;
