use crate::UiActivityStepState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiActivityStep {
    pub id: String,
    pub label: String,
    pub state: UiActivityStepState,
    pub detail: Option<String>,
}

impl UiActivityStep {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            state: UiActivityStepState::Pending,
            detail: None,
        }
    }

    pub fn with_state(mut self, state: UiActivityStepState) -> Self {
        self.state = state;
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}
