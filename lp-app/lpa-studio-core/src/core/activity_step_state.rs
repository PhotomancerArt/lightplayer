#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiActivityStepState {
    Pending,
    Active,
    Complete,
    Failed,
}

impl UiActivityStepState {
    pub fn text_marker(self) -> &'static str {
        match self {
            Self::Pending => "[ ]",
            Self::Active => "[*]",
            Self::Complete => "[x]",
            Self::Failed => "[!]",
        }
    }
}
