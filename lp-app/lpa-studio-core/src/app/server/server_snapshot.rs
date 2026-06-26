use crate::ServerState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerSnapshot {
    pub state: ServerState,
}

impl ServerSnapshot {
    pub fn new(state: ServerState) -> Self {
        Self { state }
    }
}
