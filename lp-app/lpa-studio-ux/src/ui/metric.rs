#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiMetric {
    pub label: String,
    pub value: String,
}

impl UiMetric {
    pub fn new(label: impl Into<String>, value: impl ToString) -> Self {
        Self {
            label: label.into(),
            value: value.to_string(),
        }
    }
}
