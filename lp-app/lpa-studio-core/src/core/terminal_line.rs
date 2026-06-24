//! Terminal-like text output used inside activity and workflow views.

/// A single terminal-style output line.
///
/// Use terminal lines for preformatted process output where ordering and exact
/// text matter more than structured fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiTerminalLine {
    /// Text to display for this output line.
    pub text: String,
}

impl UiTerminalLine {
    /// Create a terminal output line.
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}
