use crate::ProjectState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSnapshot {
    pub state: ProjectState,
}

impl ProjectSnapshot {
    pub fn new(state: ProjectState) -> Self {
        Self { state }
    }
}
