//! Board manifests and manifest file conversion.
//!
//! A [`HwManifest`](hw_manifest::HwManifest) is the in-memory board profile used
//! by the registry. [`HardwareManifestFile`](hw_manifest_file::HardwareManifestFile)
//! is the TOML-friendly representation used for checked-in board descriptions.

pub mod default_manifests;
pub mod hw_manifest;
pub mod hw_manifest_file;
pub mod hw_target;
