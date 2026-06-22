use crate::{UiStackSection, UiTerminalLine};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiStackView {
    pub sections: Vec<UiStackSection>,
    pub terminal: Vec<UiTerminalLine>,
}

impl UiStackView {
    pub fn new(sections: Vec<UiStackSection>) -> Self {
        Self {
            sections,
            terminal: Vec::new(),
        }
    }

    pub fn with_terminal(mut self, terminal: Vec<UiTerminalLine>) -> Self {
        self.terminal = terminal;
        self
    }
}
