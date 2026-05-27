//! Resolved source file reference (no text).

use alloc::string::String;

use lpc_model::{LpPathBuf, Revision, SourcePath};

use crate::ArtifactLoc;

/// Resolved backing for an authored [`lpc_model::SourceFileSlot`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceFileRef {
    File {
        location: ArtifactLoc,
        authored_path: SourcePath,
        resolved_path: LpPathBuf,
        extension: String,
    },
    Inline {
        extension: String,
        slot_revision: Revision,
    },
    /// URL-backed source (not supported yet).
    Url { url: String },
}
