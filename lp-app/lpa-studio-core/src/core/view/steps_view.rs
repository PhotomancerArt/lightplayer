use crate::{UiAction, UiTerminalLine, UiViewContent};

/// Render data for a multi-section workflow body.
///
/// Use steps when a pane or another view needs to show ordered workflow
/// sections, each with its own body and actions. The pane that contains the
/// workflow still owns pane title/status/actions.
#[derive(Clone, Debug, PartialEq)]
pub struct UiStepsView {
    /// Ordered workflow sections.
    pub sections: Vec<UiStepView>,
    /// Optional terminal-like output associated with the workflow.
    pub terminal: Vec<UiTerminalLine>,
}

impl UiStepsView {
    /// Create a workflow from ordered sections.
    pub fn new(sections: Vec<UiStepView>) -> Self {
        Self {
            sections,
            terminal: Vec::new(),
        }
    }

    /// Attach terminal-like output to the workflow.
    pub fn with_terminal(mut self, terminal: Vec<UiTerminalLine>) -> Self {
        self.terminal = terminal;
        self
    }
}

/// One section inside a `UiStepsView`.
#[derive(Clone, Debug, PartialEq)]
pub struct UiStepView {
    /// Stable section id.
    pub id: String,
    /// Visible section title.
    pub title: String,
    /// Current section state.
    pub state: UiStepState,
    /// Section body content.
    pub body: UiViewContent,
    /// Section-level actions.
    pub actions: Vec<UiAction>,
}

impl UiStepView {
    /// Create a step section with empty body and no actions.
    pub fn new(id: impl Into<String>, title: impl Into<String>, state: UiStepState) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            state,
            body: UiViewContent::Empty,
            actions: Vec::new(),
        }
    }

    /// Replace the section body.
    pub fn with_body(mut self, body: UiViewContent) -> Self {
        self.body = body;
        self
    }

    /// Replace the section actions.
    pub fn with_actions(mut self, actions: Vec<UiAction>) -> Self {
        self.actions = actions;
        self
    }
}

/// State for a workflow step section.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiStepState {
    /// The section is not ready to run yet.
    Pending,
    /// The section is the current active work.
    Active,
    /// The section completed successfully.
    Complete,
    /// The section needs user attention.
    NeedsAttention,
}

impl UiStepState {
    /// Return a plain-text marker for non-visual renderers and logs.
    pub fn text_marker(self) -> &'static str {
        match self {
            Self::Pending => "[ ]",
            Self::Active => "[*]",
            Self::Complete => "[x]",
            Self::NeedsAttention => "[!]",
        }
    }
}
