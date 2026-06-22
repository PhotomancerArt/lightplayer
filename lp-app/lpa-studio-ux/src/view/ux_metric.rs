#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UxMetric {
    pub label: String,
    pub value: String,
}

impl UxMetric {
    pub fn new(label: impl Into<String>, value: impl ToString) -> Self {
        Self {
            label: label.into(),
            value: value.to_string(),
        }
    }
}
