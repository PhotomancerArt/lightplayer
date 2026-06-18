use serde::{Deserialize, Serialize};

use crate::{ActionHistoryPolicy, ActionId, ActionOrigin};

/// Per-dispatch metadata attached to a Studio action.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ActionMeta {
    pub action_id: ActionId,
    pub origin: ActionOrigin,
    pub history_policy: ActionHistoryPolicy,
}

impl ActionMeta {
    pub fn new(
        action_id: ActionId,
        origin: ActionOrigin,
        history_policy: ActionHistoryPolicy,
    ) -> Self {
        Self {
            action_id,
            origin,
            history_policy,
        }
    }
}
