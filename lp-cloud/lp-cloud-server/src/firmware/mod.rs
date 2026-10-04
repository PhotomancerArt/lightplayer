//! The firmware plane: `GET|HEAD|OPTIONS /firmware/{target}/{release}/{file}`.
//!
//! A browser cannot read GitHub release assets cross-origin — neither hop of
//! `releases/download` sends `Access-Control-Allow-Origin`, the blob host
//! refuses preflights, and its signed URLs expire in an hour (measured
//! 2026-10-04, recorded in the firmware-distribution plan's notes). So this
//! server fetches released firmware from GitHub itself, checks every byte
//! against the release's `ota-manifest.json`, keeps it in the blob store by
//! SHA-256, and answers it to any origin.
//!
//! - [`firmware_route`] — the handlers and every response header.
//! - [`firmware_plane`] — the lookup: `latest` → a version, a version → its
//!   manifest, a file → verified bytes.
//! - [`firmware_manifest_cache`] — manifests, `latest` and misses, in memory,
//!   with caller-supplied time.
//! - [`firmware_upstream`] — the port the plane fetches through.
//! - [`github_release_upstream`] — that port over reqwest and
//!   `LP_CLOUD_FIRMWARE_UPSTREAM`.
//!
//! The grammar, the manifest format and verification are
//! `lpc-firmware-release`'s; nothing here re-implements them.

pub mod firmware_manifest_cache;
pub mod firmware_plane;
pub mod firmware_route;
pub mod firmware_upstream;
pub mod github_release_upstream;
