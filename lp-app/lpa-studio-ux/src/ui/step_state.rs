#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiStepState {
    Pending,
    Active,
    Complete,
    NeedsAttention,
}

impl UiStepState {
    pub fn text_marker(self) -> &'static str {
        match self {
            Self::Pending => "[ ]",
            Self::Active => "[*]",
            Self::Complete => "[x]",
            Self::NeedsAttention => "[!]",
        }
    }
}
