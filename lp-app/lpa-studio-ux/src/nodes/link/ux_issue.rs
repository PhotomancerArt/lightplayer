#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UxIssue {
    pub message: String,
    pub detail: Option<String>,
}

impl UxIssue {
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
