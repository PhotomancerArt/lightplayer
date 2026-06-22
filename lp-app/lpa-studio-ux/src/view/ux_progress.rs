#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UxProgress {
    pub label: String,
    pub detail: Option<String>,
    pub percent: Option<u32>,
    pub timeout_ms: Option<u32>,
}

impl UxProgress {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
            percent: None,
            timeout_ms: None,
        }
    }

    pub fn indeterminate(label: impl Into<String>) -> Self {
        Self::new(label)
    }

    pub fn determinate(label: impl Into<String>, percent: u32) -> Self {
        Self::new(label).with_percent(percent)
    }

    pub fn timeout(label: impl Into<String>, timeout_ms: u32) -> Self {
        Self::new(label).with_timeout_ms(timeout_ms)
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_percent(mut self, percent: u32) -> Self {
        self.percent = Some(percent.min(100));
        self
    }

    pub fn with_timeout_ms(mut self, timeout_ms: u32) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }
}
