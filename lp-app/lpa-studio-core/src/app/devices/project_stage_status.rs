//! Where a board's project stage stands (M3 P11), as the card reads it when
//! no Update runs the stage: running with no click, or held back for a
//! reason a person can act on. Facts, not words: the card's words are
//! [`super::device_update_words`]'s.
//!
//! The project stage itself is `lpa-update`'s ([`lpa_update::ProjectStage`]);
//! [`ProjectStageStatus::after`] reads its report.

use lpa_update::{ProjectStageReport, StageEnd};

/// The board's project stage, as the card says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectStageStatus {
    /// Rewriting: `done` of `total` projects older than the engine's format.
    Running { done: u32, total: u32 },
    /// Older projects are left on the board: `why`.
    HeldBack(ProjectsHeldBack),
}

/// Why a board's older projects were left as they are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectsHeldBack {
    /// The link holds less than edit (Q103): someone who can edit finishes.
    NeedsEdit,
    /// This Studio's upgrader stops short of the board's engine format
    /// (Q105): a reload of Studio finishes.
    StudioTooOld,
    /// The step refused `project` (Q104) with its own sentence `why`; the
    /// other projects were upgraded.
    Refused { project: String, why: String },
    /// The board's files cannot be rewritten in one transaction (a board
    /// without the tree store): nothing was written.
    NotAtomic,
}

impl ProjectStageStatus {
    /// What the card says after a stage ended with `report`: `None` when
    /// nothing is left for a person (every project is at the engine's
    /// format), or when the stage was cut short and the next one finishes
    /// it (an interrupted stage resumes from the board).
    pub fn after(report: &ProjectStageReport) -> Option<Self> {
        let held = match &report.end {
            StageEnd::NeedsEdit => ProjectsHeldBack::NeedsEdit,
            StageEnd::HostTooOld(_) => ProjectsHeldBack::StudioTooOld,
            StageEnd::NotAtomic => ProjectsHeldBack::NotAtomic,
            StageEnd::Interrupted(_) => return None,
            StageEnd::Finished => {
                let refused = report.refused.first()?;
                ProjectsHeldBack::Refused {
                    project: project_name(&refused.project).to_string(),
                    why: refused.why.clone(),
                }
            }
        };
        Some(Self::HeldBack(held))
    }
}

/// A listed project's name: the last segment of its path.
pub fn project_name(project: &str) -> &str {
    let trimmed = project.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_update::RefusedProject;

    #[test]
    fn a_finished_stage_leaves_nothing_unless_a_project_was_refused() {
        let done = ProjectStageReport::default();
        assert_eq!(ProjectStageStatus::after(&done), None);
        let refused = ProjectStageReport {
            refused: vec![RefusedProject {
                project: "projects/eaves".to_string(),
                why: "a sentence".to_string(),
            }],
            ..ProjectStageReport::default()
        };
        assert_eq!(
            ProjectStageStatus::after(&refused),
            Some(ProjectStageStatus::HeldBack(ProjectsHeldBack::Refused {
                project: "eaves".to_string(),
                why: "a sentence".to_string(),
            }))
        );
    }

    #[test]
    fn an_interrupted_stage_says_nothing_and_a_held_one_says_why() {
        let cut = ProjectStageReport {
            end: StageEnd::Interrupted("the link dropped".to_string()),
            ..ProjectStageReport::default()
        };
        assert_eq!(ProjectStageStatus::after(&cut), None);
        let play = ProjectStageReport {
            end: StageEnd::NeedsEdit,
            ..ProjectStageReport::default()
        };
        assert_eq!(
            ProjectStageStatus::after(&play),
            Some(ProjectStageStatus::HeldBack(ProjectsHeldBack::NeedsEdit))
        );
    }
}
