use serde::{Deserialize, Serialize};

/// Source that initiated a Studio ux.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ActionOrigin {
    User,
    Agent,
    Harness,
    System,
}
