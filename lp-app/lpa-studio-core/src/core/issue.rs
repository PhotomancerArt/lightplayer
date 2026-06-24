#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiIssue {
    pub message: String,
    pub detail: Option<String>,
}

impl UiIssue {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}
