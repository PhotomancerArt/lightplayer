//! **The project stage**: the update's last stage, upgrading a board's
//! stored projects to the format its engine reads, over channel 1, once that
//! engine answers there (M3 P10; notes "M3 amended for G1", Q99, Q114).
//!
//! A sibling of the update driver, not part of it: [`crate::UpdateDriver`]
//! and its decision table never see a project. The stage runs after the
//! driver's `Done(UpToDate)`, and on its own whenever a board is found whose
//! engine reads a newer format than a project it stores (Q107).
//!
//! - [`project_stage`]: the session ([`ProjectStage`]);
//! - [`project_stage_effect`]: what it asks the edge to do;
//! - [`project_stage_report`]: how it ended, project by project;
//! - [`stored_project`]: one project as the stage found it on the board.
//!
//! General by construction: the stage knows the board's engine format, the
//! projects' `project.json` formats and `lpa-upgrade`'s chain, and nothing
//! of what any step changes. A later format's step rides it with no new
//! work (`tests/projects_stage.rs` proves that with a toy v12→v13).

pub mod project_stage;
pub mod project_stage_effect;
pub mod project_stage_report;
pub mod stored_project;

pub use project_stage::ProjectStage;
pub use project_stage_effect::{BatchOutcome, ProjectStageEffect};
pub use project_stage_report::{ProjectStageReport, RefusedProject, StageEnd, UpgradedProject};
pub use stored_project::{StoredDisposition, StoredProject};
