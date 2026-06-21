use serde::{Deserialize, Serialize};

use crate::{AvailableAction, ServerActionRequest};

/// User-facing state of the `lp-server` protocol connection.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub enum ServerState {
    #[default]
    WaitingForLink,
    Connecting,
    Opening,
    ReadingStatus,
    Ready,
    RecoveryRequired {
        reason: String,
    },
    Degraded,
    Disconnected,
}

impl ServerState {
    pub fn available_actions(&self) -> Vec<AvailableAction<ServerActionRequest>> {
        match self {
            Self::Ready => vec![
                available(ServerActionRequest::RefreshStatus).tertiary(),
                available(ServerActionRequest::ReadProjectState),
            ],
            Self::RecoveryRequired { .. } | Self::Degraded => {
                vec![available(ServerActionRequest::RefreshStatus)]
            }
            _ => Vec::new(),
        }
    }
}

fn available(action: ServerActionRequest) -> AvailableAction<ServerActionRequest> {
    AvailableAction::new(action.clone(), action.action_type().into())
}
