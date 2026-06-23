use crate::UiStatusKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiStatus {
    pub label: String,
    pub kind: UiStatusKind,
}

impl UiStatus {
    pub fn new(label: impl Into<String>, kind: UiStatusKind) -> Self {
        Self {
            label: label.into(),
            kind,
        }
    }

    pub fn neutral(label: impl Into<String>) -> Self {
        Self::new(label, UiStatusKind::Neutral)
    }

    pub fn working(label: impl Into<String>) -> Self {
        Self::new(label, UiStatusKind::Working)
    }

    pub fn good(label: impl Into<String>) -> Self {
        Self::new(label, UiStatusKind::Good)
    }

    pub fn warning(label: impl Into<String>) -> Self {
        Self::new(label, UiStatusKind::Warning)
    }

    pub fn error(label: impl Into<String>) -> Self {
        Self::new(label, UiStatusKind::Error)
    }
}
