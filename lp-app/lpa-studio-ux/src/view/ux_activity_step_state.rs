#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UxActivityStepState {
    Pending,
    Active,
    Complete,
    Failed,
}

impl UxActivityStepState {
    pub fn text_marker(self) -> &'static str {
        match self {
            Self::Pending => "[ ]",
            Self::Active => "[*]",
            Self::Complete => "[x]",
            Self::Failed => "[!]",
        }
    }
}
