use serde::{Deserialize, Serialize};

use crate::StudioActionType;

/// User or agent intent owned by the Studio project manager.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ProjectActionRequest {
    UploadDemoProject,
    LoadDemoProject,
    ReadProjectInventory,
    SelectProjectNode { node_id: Option<String> },
}

impl ProjectActionRequest {
    pub fn action_type(&self) -> StudioActionType {
        match self {
            Self::UploadDemoProject => StudioActionType::UploadDemoProject,
            Self::LoadDemoProject => StudioActionType::LoadDemoProject,
            Self::ReadProjectInventory => StudioActionType::ReadProjectInventory,
            Self::SelectProjectNode { .. } => StudioActionType::SelectProjectNode,
        }
    }
}
