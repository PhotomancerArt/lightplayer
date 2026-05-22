//! Resolved source file reference (no text).

use alloc::string::String;

use lpc_model::{LpPathBuf, Revision, SourcePath};

use crate::ArtifactId;

/// Resolved backing for an authored [`lpc_model::SourceFileSlot`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceFileRef {
    File {
        artifact_id: ArtifactId,
        authored_path: SourcePath,
        resolved_path: LpPathBuf,
        extension: String,
    },
    Inline {
        extension: String,
        slot_revision: Revision,
    },
    /// Future URL-backed source (unsupported in M3).
    Url { url: String },
}
