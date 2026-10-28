//! How the project stage ended, project by project. Facts, not copy
//! (DM31): the caller words them, except a refusal's sentence, which is the
//! upgrader's own.

use alloc::string::String;
use alloc::vec::Vec;

use lpa_upgrade::{UpgradeError, UpgradeReport};

/// What the stage did on one board.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectStageReport {
    /// The project format the board's engine reads (its hello's
    /// `build.projectFormat`).
    pub engine_format: u32,
    /// The startup project, when the board lists any project.
    pub startup: Option<String>,
    /// Rewritten and committed, in the order written (the startup project
    /// first).
    pub upgraded: Vec<UpgradedProject>,
    /// Left as they are, each with the upgrader's sentence.
    pub refused: Vec<RefusedProject>,
    /// Already at the engine's format.
    pub untouched: Vec<String>,
    /// Older than the engine's format and not (yet) rewritten: what a
    /// stage that ended early leaves for the next one.
    pub pending: Vec<String>,
    /// The startup project's load, when the stage asked for one: `Ok`, or
    /// the board's words.
    pub load: Option<Result<(), String>>,
    /// Why it ended.
    pub end: StageEnd,
}

/// One project the stage rewrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradedProject {
    pub project: String,
    /// The upgrader's account: formats, changed, added and removed files.
    pub report: UpgradeReport,
}

/// One project the stage left alone because it could not upgrade it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefusedProject {
    pub project: String,
    /// The upgrader's (or the classifier's) own sentence.
    pub why: String,
}

/// Why the stage ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum StageEnd {
    /// Every stored project the steps accept is at the engine's format
    /// (including "there was nothing to do").
    #[default]
    Finished,
    /// Older projects were found and the link holds less than edit:
    /// nothing was written (Q103). `pending` names them.
    NeedsEdit,
    /// This host's upgrader stops short of the engine's format (Q105):
    /// nothing was read. The error's sentence says to update the host.
    HostTooOld(UpgradeError),
    /// The board's filesystem cannot rewrite a project in one transaction
    /// (its `BeginBatch` answered `atomic: false`): nothing was written.
    NotAtomic,
    /// A read or a batch failed, or the link dropped: every project is
    /// wholly old or wholly new, and a stage run again finishes `pending`.
    Interrupted(String),
}
