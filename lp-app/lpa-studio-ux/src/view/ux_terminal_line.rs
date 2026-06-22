#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UxTerminalLine {
    pub text: String,
}

impl UxTerminalLine {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}
