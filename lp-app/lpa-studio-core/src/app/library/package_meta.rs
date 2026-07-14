//! The provenance sidecar `/.lp/meta.json`.
//!
//! Lives inside the package (so it travels on export/pull) but under the
//! reserved `/.lp/` namespace, which the lph1 hash spec excludes — metadata
//! churn never changes a package's content hash.

use lpc_model::AsLpPath;
use lpfs::LpFs;
use serde::{Deserialize, Serialize};

use super::library_store::LibraryError;

pub const META_PATH: &str = "/.lp/meta.json";

/// Where a package came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PackageProvenance {
    /// Created from scratch in this library.
    Created,
    /// Seeded from a bundled source (e.g. `examples/basic`).
    SeededFrom { source: String },
    /// Imported from a zip archive; the archive's own uid, if it had one.
    ImportedZip { original_uid: Option<String> },
    /// Forked from another project's version.
    ForkedFrom {
        parent_project: String,
        parent_version: String,
    },
    /// Adopted from a device carrying a project this library did not know
    /// (connect-as-pull, D8/D11).
    PulledFromDevice {
        device_uid: String,
        device_name: String,
    },
}

/// The sidecar contents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageMeta {
    pub provenance: PackageProvenance,
    /// f64 epoch seconds, caller-supplied.
    pub created_at: f64,
}

pub fn read_meta(fs: &dyn LpFs) -> Result<Option<PackageMeta>, LibraryError> {
    if !fs
        .file_exists(META_PATH.as_path())
        .map_err(|e| LibraryError::Meta(format!("{e}")))?
    {
        return Ok(None);
    }
    let bytes = fs
        .read_file(META_PATH.as_path())
        .map_err(|e| LibraryError::Meta(format!("read meta: {e}")))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| LibraryError::Meta(format!("parse meta: {e}")))
}

pub fn write_meta(fs: &dyn LpFs, meta: &PackageMeta) -> Result<(), LibraryError> {
    let bytes = serde_json::to_vec_pretty(meta)
        .map_err(|e| LibraryError::Meta(format!("serialize meta: {e}")))?;
    fs.write_file(META_PATH.as_path(), &bytes)
        .map_err(|e| LibraryError::Meta(format!("write meta: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::LpFsMemory;

    #[test]
    fn round_trips_and_is_absent_by_default() {
        let fs = LpFsMemory::new();
        assert!(read_meta(&fs).unwrap().is_none());
        let meta = PackageMeta {
            provenance: PackageProvenance::SeededFrom {
                source: "examples/basic".to_string(),
            },
            created_at: 1700000000.5,
        };
        write_meta(&fs, &meta).unwrap();
        assert_eq!(read_meta(&fs).unwrap().unwrap(), meta);
    }
}
