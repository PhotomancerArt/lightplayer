use crate::UxActivityStepState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UxActivityStep {
    pub id: String,
    pub label: String,
    pub state: UxActivityStepState,
    pub detail: Option<String>,
}

impl UxActivityStep {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            state: UxActivityStepState::Pending,
            detail: None,
        }
    }

    pub fn with_state(mut self, state: UxActivityStepState) -> Self {
        self.state = state;
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}
