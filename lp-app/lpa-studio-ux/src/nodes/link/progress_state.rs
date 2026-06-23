#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressState {
    pub label: String,
    pub detail: Option<String>,
}

impl ProgressState {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}
