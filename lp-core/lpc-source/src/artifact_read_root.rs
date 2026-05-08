//! [`crate::ArtifactReadRoot`] for [`lpfs::lp_fs::LpFs`] implementations.

use crate::ArtifactReadRoot;
use lpfs::error::FsError;
use lpfs::lp_path::LpPath;
use lpfs::{LpFs, LpFsMemory, LpFsView};

macro_rules! artifact_read_root {
    ($t:ty) => {
        impl ArtifactReadRoot for $t {
            type Err = FsError;

            fn read_file(&self, path: &LpPath) -> Result<alloc::vec::Vec<u8>, FsError> {
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
