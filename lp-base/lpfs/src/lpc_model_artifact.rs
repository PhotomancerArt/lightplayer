//! [`lpc_source::ArtifactReadRoot`] for [`crate::LpFs`] implementations.

use crate::error::FsError;
use crate::{LpFs, LpFsMemory, LpFsView};
use lpc_model::lp_path::LpPath;
use lpc_source::ArtifactReadRoot;

macro_rules! impl_artifact_read_root {
    ($t:ty) => {
        impl ArtifactReadRoot for $t {
            type Err = FsError;

            fn read_file(&self, path: &LpPath) -> Result<alloc::vec::Vec<u8>, FsError> {
                LpFs::read_file(self, path)
            }
        }
    };
}

impl_artifact_read_root!(LpFsMemory);
impl_artifact_read_root!(LpFsView);
impl_artifact_read_root!(dyn LpFs);

#[cfg(feature = "std")]
impl_artifact_read_root!(crate::LpFsStd);
