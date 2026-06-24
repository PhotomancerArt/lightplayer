use crate::UiProgress;

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

impl From<ProgressState> for UiProgress {
    fn from(progress: ProgressState) -> Self {
        let mut ui_progress = UiProgress::indeterminate(progress.label);
        if let Some(detail) = progress.detail {
            ui_progress = ui_progress.with_detail(detail);
        }
        ui_progress
    }
}
