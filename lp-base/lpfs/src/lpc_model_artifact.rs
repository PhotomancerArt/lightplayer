//! [`lpc_model::ArtifactReadRoot`] for [`crate::LpFs`] implementations.

use crate::LpFs;
use crate::error::FsError;
use lpc_model::ArtifactReadRoot;
use lpc_model::path::LpPath;

impl<T: LpFs + ?Sized> ArtifactReadRoot for T {
    type Err = FsError;

    fn read_file(&self, path: &LpPath) -> Result<alloc::vec::Vec<u8>, FsError> {
        LpFs::read_file(self, path)
    }
}
