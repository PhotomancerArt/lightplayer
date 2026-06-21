use serde::{Deserialize, Serialize};

use crate::{AvailableAction, ProjectActionRequest, ProjectChoice, ProjectSelectionReason};

/// User-facing state of Studio's attachment to a project on a ready server.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub enum ProjectState {
    #[default]
    WaitingForServer,
    ReadingServerProjects,
    ProjectSelectionRequired {
        reason: ProjectSelectionReason,
        projects: Vec<ProjectChoice>,
    },
    Attaching,
    Loading,
    Ready {
        project_id: String,
        sync: ProjectSyncState,
    },
    Resyncing {
        project_id: String,
    },
    Deploying,
    Degraded {
        message: String,
    },
    Detached,
}

impl ProjectState {
    pub fn available_actions(&self) -> Vec<AvailableAction<ProjectActionRequest>> {
        match self {
            Self::ProjectSelectionRequired { .. } => vec![
                available(ProjectActionRequest::LoadDemoProject).primary(),
                available(ProjectActionRequest::UploadDemoProject),
            ],
            Self::Ready { .. } => vec![
                available(ProjectActionRequest::ReadProjectInventory).tertiary(),
                available(ProjectActionRequest::LoadDemoProject),
            ],
            Self::Detached => vec![available(ProjectActionRequest::LoadDemoProject).primary()],
            _ => Vec::new(),
        }
    }
}

/// Nested sync status for a ready project.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ProjectSyncState {
    Clean,
    Dirty,
    Saving,
    Deploying,
    Conflict,
    Unknown,
}

fn available(action: ProjectActionRequest) -> AvailableAction<ProjectActionRequest> {
    AvailableAction::new(action.clone(), action.action_type().into())
}
