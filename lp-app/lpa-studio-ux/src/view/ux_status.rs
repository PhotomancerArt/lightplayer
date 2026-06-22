use crate::UxStatusKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UxStatus {
    pub label: String,
    pub kind: UxStatusKind,
}

impl UxStatus {
    pub fn new(label: impl Into<String>, kind: UxStatusKind) -> Self {
        Self {
            label: label.into(),
            kind,
        }
    }

    pub fn neutral(label: impl Into<String>) -> Self {
        Self::new(label, UxStatusKind::Neutral)
    }

    pub fn working(label: impl Into<String>) -> Self {
        Self::new(label, UxStatusKind::Working)
    }

    pub fn good(label: impl Into<String>) -> Self {
        Self::new(label, UxStatusKind::Good)
    }

    pub fn warning(label: impl Into<String>) -> Self {
        Self::new(label, UxStatusKind::Warning)
    }

    pub fn error(label: impl Into<String>) -> Self {
        Self::new(label, UxStatusKind::Error)
    }
}
