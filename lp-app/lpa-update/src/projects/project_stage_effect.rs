//! What the project stage asks its edge to do, and what a batch came to.

use alloc::string::String;
use alloc::vec::Vec;

use crate::projects::project_stage_report::ProjectStageReport;

/// One thing for the edge to do on the board's channel 1, then answer the
/// stage with the matching input. The stage asks for one at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectStageEffect {
    /// `ClientRequest::ListAvailableProjects`; answer
    /// [`crate::ProjectStage::projects_listed`] with the paths.
    ListProjects,
    /// `FsRequest::Read` of `/lightplayer.json` (the board's startup
    /// choice); answer [`crate::ProjectStage::startup_choice_read`] with its
    /// bytes, or `None` when the board has none.
    ReadStartupChoice,
    /// `FsRequest::Read` of `<project>/project.json`; answer
    /// [`crate::ProjectStage::manifest_read`].
    ReadManifest { project: String },
    /// Every file under `project` (`FsRequest::ListDir` recursive, then a
    /// `Read` each, the project's `.lp/` included), keyed by its path inside
    /// the project with no leading `/`; answer
    /// [`crate::ProjectStage::project_read`].
    ReadProject { project: String },
    /// One atomic rewrite of `project`: `BeginBatch`; **if the board
    /// answers `atomic: false`, write nothing** and answer
    /// [`BatchOutcome::NotAtomic`]; else `Write` each of `writes` (paths
    /// inside the project, no leading `/`), `DeleteFile` each of `deletes`,
    /// and `CommitBatch`. Answer [`crate::ProjectStage::batch_done`].
    WriteBatch {
        project: String,
        writes: Vec<(String, Vec<u8>)>,
        deletes: Vec<String>,
    },
    /// `ClientRequest::LoadProject` of the startup project, now that it is
    /// at the engine's format; answer [`crate::ProjectStage::load_done`].
    LoadProject { project: String },
    /// The stage is over: say what it did (the caller words it).
    Done(ProjectStageReport),
}

/// How a [`ProjectStageEffect::WriteBatch`] ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BatchOutcome {
    /// `CommitBatch` landed: the project is wholly at the engine's format.
    Committed,
    /// `BeginBatch` answered `atomic: false` (a filesystem with no
    /// transactions, the classic's): nothing was written.
    NotAtomic,
    /// A write, a delete or the commit failed (or the link dropped): the
    /// batch did not land, so the project is wholly as it was.
    Failed(String),
}
