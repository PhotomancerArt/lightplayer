//! What a project-stage conversation posts back onto the actor's queue
//! ([`super::project_stage_controller`]): how far it is, and how it ended.

use lpa_devices::identity::DeviceId;
use lpa_update::ProjectStageReport;

/// One input to the project-stage controller, from a stage it spawned.
/// `generation` names the run, so a straggler from a run since replaced is
/// dropped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectStageCommand {
    /// The stage is rewriting project `done + 1` of `total` (counted in
    /// projects older than the board's engine format).
    Progress {
        device: DeviceId,
        generation: u64,
        done: u32,
        total: u32,
    },
    /// The stage ended.
    Done {
        device: DeviceId,
        generation: u64,
        report: ProjectStageReport,
    },
}
