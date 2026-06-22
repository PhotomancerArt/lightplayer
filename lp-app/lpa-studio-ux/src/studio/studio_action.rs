use crate::{ActionKind, LinkAction, ProjectAction, UxCommand};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StudioAction {
    Link(LinkAction),
    Project(ProjectAction),
}

impl From<LinkAction> for StudioAction {
    fn from(action: LinkAction) -> Self {
        Self::Link(action)
    }
}

impl From<ProjectAction> for StudioAction {
    fn from(action: ProjectAction) -> Self {
        Self::Project(action)
    }
}

impl UxCommand for StudioAction {
    fn action_kind(&self) -> ActionKind {
        match self {
            Self::Link(action) => action.action_kind(),
            Self::Project(action) => action.action_kind(),
        }
    }
}
