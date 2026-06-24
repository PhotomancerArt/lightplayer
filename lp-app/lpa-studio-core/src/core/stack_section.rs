use crate::{UiAction, UiBody, UiStepState};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiStackSection {
    pub id: String,
    pub title: String,
    pub state: UiStepState,
    pub body: UiBody,
    pub actions: Vec<UiAction>,
}

impl UiStackSection {
    pub fn new(id: impl Into<String>, title: impl Into<String>, state: UiStepState) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            state,
            body: UiBody::Empty,
            actions: Vec::new(),
        }
    }

    pub fn with_body(mut self, body: UiBody) -> Self {
        self.body = body;
        self
    }

    pub fn with_actions(mut self, actions: Vec<UiAction>) -> Self {
        self.actions = actions;
        self
    }
}
