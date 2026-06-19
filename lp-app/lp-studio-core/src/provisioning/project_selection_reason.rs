use serde::{Deserialize, Serialize};

/// Why Studio needs user intent before it can attach to a project.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ProjectSelectionReason {
    NoLoadedProject,
    MultipleLoadedProjects,
}
