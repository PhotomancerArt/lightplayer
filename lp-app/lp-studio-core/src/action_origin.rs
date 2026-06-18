use serde::{Deserialize, Serialize};

/// Source that initiated a Studio action.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ActionOrigin {
    User,
    Agent,
    Harness,
    System,
}
