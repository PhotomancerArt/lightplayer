//! Filesystem read surface for authored artifact loading.

use alloc::vec::Vec;

use lpfs::error::FsError;
use lpfs::lp_path::LpPath;
use lpfs::{LpFs, LpFsMemory, LpFsView};

/// Narrow filesystem surface used by artifact loaders.
pub trait ArtifactReadRoot {
    /// Low-level error returned when reading bytes fails.
    type Err;

    /// Read full file contents at `path`.
    fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, Self::Err>;
}

macro_rules! artifact_read_root {
    ($t:ty) => {
        impl ArtifactReadRoot for $t {
            type Err = FsError;

            fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, FsError> {
                LpFs::read_file(self, path)
            }
        }
    };
}

artifact_read_root!(LpFsMemory);
artifact_read_root!(LpFsView);
artifact_read_root!(dyn LpFs);

#[cfg(feature = "std")]
artifact_read_root!(lpfs::LpFsStd);
