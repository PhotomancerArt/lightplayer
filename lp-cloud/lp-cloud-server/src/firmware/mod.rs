//! The firmware plane: `GET|HEAD|OPTIONS /firmware/{target}/{release}/{file}`
//! and the release index, `GET|HEAD|OPTIONS
//! /api/v1/firmware/{target}/releases`.
//!
//! A browser cannot read GitHub release assets cross-origin — neither hop of
//! `releases/download` sends `Access-Control-Allow-Origin`, the blob host
//! refuses preflights, and its signed URLs expire in an hour (measured
//! 2026-10-04, recorded in the firmware-distribution plan's notes). So this
//! server fetches released firmware from GitHub itself, checks every byte
//! against the release's `ota-manifest.json`, keeps it in the blob store by
//! SHA-256, and answers it to any origin.
//!
//! - [`firmware_route`] — the lookup's handlers and every response header.
//! - [`firmware_plane`] — the lookup: `latest` → a version, a version → its
//!   manifest, a file → verified bytes.
//! - [`firmware_manifest_cache`] — manifests, `latest` and misses, in memory,
//!   with caller-supplied time.
//! - [`firmware_upstream`] — the port the plane fetches files through.
//! - [`github_release_upstream`] — that port over reqwest and
//!   `LP_CLOUD_FIRMWARE_UPSTREAM`.
//! - [`firmware_index_route`] — the release index's handler and headers.
//! - [`release_index_plane`] — the index: the releases list, filtered to
//!   one target's complete releases, from their verified manifests.
//! - [`release_index_cache`] — the list (ETag, TTL, stale-on-error) and
//!   each target's rendered index, with caller-supplied time.
//! - [`release_list_upstream`] — the port the list is fetched through.
//! - [`github_release_list_upstream`] — that port over reqwest,
//!   `LP_CLOUD_FIRMWARE_RELEASES_LIST` and the optional
//!   `LP_CLOUD_GITHUB_TOKEN`.
//! - [`github_release_list`] — reading GitHub's releases JSON.
//!
//! The grammar, the manifest and index formats and verification are
//! `lpc-firmware-release`'s; nothing here re-implements them.

pub mod firmware_index_route;
pub mod firmware_manifest_cache;
pub mod firmware_plane;
pub mod firmware_route;
pub mod firmware_upstream;
pub mod github_release_list;
pub mod github_release_list_upstream;
pub mod github_release_upstream;
pub mod release_index_cache;
pub mod release_index_plane;
pub mod release_list_upstream;
