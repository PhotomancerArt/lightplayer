//! Small label/value facts for summary surfaces.

/// A compact label/value pair for scannable UI summaries.
///
/// Use metrics for facts that should be compared quickly, such as protocol,
/// runtime, frame rate, memory, or connection details.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiMetric {
    /// Human-readable metric label.
    pub label: String,
    /// Human-readable metric value.
    pub value: String,
}

impl UiMetric {
    /// Create a metric from a label and any displayable value.
    pub fn new(label: impl Into<String>, value: impl ToString) -> Self {
        Self {
            label: label.into(),
            value: value.to_string(),
        }
    }
}
