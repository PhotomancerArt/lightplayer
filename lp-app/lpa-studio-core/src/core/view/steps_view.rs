use crate::{UiAction, UiTerminalLine, UiViewContent};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiStepsView {
    pub sections: Vec<UiStepView>,
    pub terminal: Vec<UiTerminalLine>,
}

impl UiStepsView {
    pub fn new(sections: Vec<UiStepView>) -> Self {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiStepView {
    pub id: String,
    pub title: String,
    pub state: UiStepState,
    pub body: UiViewContent,
    pub actions: Vec<UiAction>,
}

impl UiStepView {
    pub fn new(id: impl Into<String>, title: impl Into<String>, state: UiStepState) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            state,
            body: UiViewContent::Empty,
            actions: Vec::new(),
        }
    }

    pub fn with_body(mut self, body: UiViewContent) -> Self {
        self.body = body;
        self
    }

    pub fn with_actions(mut self, actions: Vec<UiAction>) -> Self {
        self.actions = actions;
        self
    }
}

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
