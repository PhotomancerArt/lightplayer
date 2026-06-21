use serde::{Deserialize, Serialize};

use crate::StudioActionType;

/// User or agent intent owned by the Studio server manager.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ServerActionRequest {
    RefreshStatus,
    ReadProjectState,
}

impl ServerActionRequest {
    pub fn action_type(&self) -> StudioActionType {
        match self {
            Self::RefreshStatus => StudioActionType::RefreshStatus,
            Self::ReadProjectState => StudioActionType::ReadProjectState,
        }
    }
}
