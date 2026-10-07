//! The firmware distribution contract, shared by the three edges that touch
//! released firmware: `lp-cli` writes it, `lp-cloud-server` serves it, and
//! Studio (and the update protocol's host crate) read it.
//!
//! - [`OtaManifest`] — `ota-manifest.json` **format 1** (one target's build
//!   in one release), its compatibility rule and its [`encodings`
//!   ](OtaManifest::encodings) chosen by id.
//! - [`asset_name`] — release asset names, `<target>.<file>` on the tag
//!   `v<version>`.
//! - [`FirmwareLookupPath`] / [`ReleaseSelector`] — the lookup URL
//!   `/firmware/<target>/<release>/<file>`, with `latest` and the reserved
//!   selector words.
//! - [`OtaManifest::files`] / [`OtaManifest::verify`] — the file allowlist
//!   and byte verification.
//! - [`ReleaseIndex`] — the release index **format 1** (every release one
//!   target can install, newest first), served at
//!   [`release_index_path`] `/api/v1/firmware/<target>/releases`.
//! - [`ReleaseVersion`] — ordered by number (`-10` is newer than `-9`).
//!
//! Names (N1, doors #1): a **target** is a line of builds (`esp32c6-4mb`,
//! opaque), a **version** is `2026.10.05-3`, and a **build id** is
//! `<version>+<commit[..12]>` (JSON key `buildId`).
//!
//! `no_std` + `alloc`, sans-IO: no clock, no filesystem, no HTTP. What
//! encoding 1 *means* (the dictionary rule) is the update protocol's
//! (`lpc-update`); this crate only describes encodings and checks their
//! structure.

#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod dev_version;
mod firmware_file_check;
mod firmware_lookup_path;
mod lookup_error;
mod lower_hex;
mod ota_encoding;
mod ota_manifest;
mod ota_manifest_error;
mod release_asset_name;
mod release_index;
mod release_index_error;
mod release_index_path;
mod release_selector;
mod release_version;
mod target_name;

pub use dev_version::{APP_VERSION_MAX_LEN, is_app_version, is_dev_version};
pub use firmware_file_check::{FirmwareFileError, FirmwareFileRef};
pub use firmware_lookup_path::{
    FILE_NAME_MAX_LEN, FIRMWARE_PATH_PREFIX, FirmwareLookupPath, OTA_MANIFEST_FILE,
    is_lookup_file_name,
};
pub use lookup_error::LookupError;
pub use lower_hex::{
    BUILD_ID_COMMIT_DIGITS, COMMIT_HEX_LEN, SHA256_HEX_LEN, is_lower_hex, sha256_hex,
};
pub use ota_encoding::{ENCODING_DEFLATE_DICT_V1, EncodedPieceFile, Encoding1, EncodingEntry};
pub use ota_manifest::{OTA_MANIFEST_FORMAT, OtaManifest, PackageRef, PieceFile, Requires};
pub use ota_manifest_error::OtaManifestError;
pub use release_asset_name::{asset_name, split_asset_name};
pub use release_index::{RELEASE_INDEX_FORMAT, ReleaseIndex, ReleaseIndexEntry};
pub use release_index_error::ReleaseIndexError;
pub use release_index_path::{
    RELEASE_INDEX_PATH_PREFIX, RELEASE_INDEX_SEGMENT, parse_release_index_path, release_index_path,
};
pub use release_selector::{LATEST, ReleaseSelector};
pub use release_version::{BuildId, ReleaseVersion, is_release_version};
pub use target_name::{TARGET_NAME_MAX_LEN, TargetName, is_target_name};
