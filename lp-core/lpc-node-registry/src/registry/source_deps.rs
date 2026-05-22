//! Resolved source file dependencies tracked per def entry.

use lpc_model::Revision;
use lpfs::LpPathBuf;

/// One file-backed source path and its last materialized version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDep {
    pub resolved_path: LpPathBuf,
    pub last_version: Revision,
}
