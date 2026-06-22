#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiTerminalLine {
    pub text: String,
}

impl UiTerminalLine {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}
