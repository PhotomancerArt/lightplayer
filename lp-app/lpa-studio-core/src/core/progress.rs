//! Render-ready progress state for ongoing work.

/// Progress metadata for a visible operation.
///
/// Use this when a view should show ongoing work. `percent` means determinate
/// progress; `timeout_ms` means a known countdown/window; neither means
/// indeterminate progress.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiProgress {
    /// Short label for the operation.
    pub label: String,
    /// Optional supporting context.
    pub detail: Option<String>,
    /// Optional determinate completion percentage, clamped to 100.
    pub percent: Option<u32>,
    /// Optional timeout/countdown duration in milliseconds.
    pub timeout_ms: Option<u32>,
}

impl UiProgress {
    /// Create indeterminate progress with a label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
            percent: None,
            timeout_ms: None,
        }
    }

    /// Create indeterminate progress with a label.
    pub fn indeterminate(label: impl Into<String>) -> Self {
        Self::new(label)
    }

    /// Create determinate progress with a percentage.
    pub fn determinate(label: impl Into<String>, percent: u32) -> Self {
        Self::new(label).with_percent(percent)
    }

    /// Create timeout progress for an operation with a known wait window.
    pub fn timeout(label: impl Into<String>, timeout_ms: u32) -> Self {
        Self::new(label).with_timeout_ms(timeout_ms)
    }

    /// Attach supporting detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Attach a determinate percentage, clamped to 100.
    pub fn with_percent(mut self, percent: u32) -> Self {
        self.percent = Some(percent.min(100));
        self
    }

    /// Attach a timeout/countdown duration in milliseconds.
    pub fn with_timeout_ms(mut self, timeout_ms: u32) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }
}
