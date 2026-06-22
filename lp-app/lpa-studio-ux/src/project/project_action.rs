use crate::{ActionKind, UxCommand};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectAction {
    LoadDemoProject,
}

impl ProjectAction {
    pub const LOAD_DEMO_PROJECT: ActionKind = ActionKind::new("project", "load-demo-project");
}

impl UxCommand for ProjectAction {
    fn action_kind(&self) -> ActionKind {
        match self {
            Self::LoadDemoProject => Self::LOAD_DEMO_PROJECT,
        }
    }
}
