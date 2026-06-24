use crate::{UiProgress, UiTerminalLine};

/// Render data for a multi-step activity currently in progress.
///
/// Use an activity when a pane body needs to show a named operation, optional
/// progress, step-by-step state, and optional terminal output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiActivityView {
    /// Activity title.
    pub title: String,
    /// Optional supporting detail.
    pub detail: Option<String>,
    /// Optional progress indicator for the activity as a whole.
    pub progress: Option<UiProgress>,
    /// Ordered activity steps.
    pub steps: Vec<UiActivityStep>,
    /// Recent terminal-like output associated with the activity.
    pub terminal: Vec<UiTerminalLine>,
}

impl UiActivityView {
    /// Create an activity with a title and no detail, progress, steps, or output.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            detail: None,
            progress: None,
            steps: Vec::new(),
            terminal: Vec::new(),
        }
    }

    /// Attach supporting detail to the activity.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Attach progress for the activity as a whole.
    pub fn with_progress(mut self, progress: UiProgress) -> Self {
        self.progress = Some(progress);
        self
    }

    /// Replace the activity steps.
    pub fn with_steps(mut self, steps: Vec<UiActivityStep>) -> Self {
        self.steps = steps;
        self
    }

    /// Replace the terminal output lines.
    pub fn with_terminal(mut self, terminal: Vec<UiTerminalLine>) -> Self {
        self.terminal = terminal;
        self
    }

    /// Update a step by id if it exists.
    pub fn set_step_state(&mut self, id: &str, state: UiActivityStepState) {
        if let Some(step) = self.steps.iter_mut().find(|step| step.id == id) {
            step.state = state;
        }
    }

    /// Append a terminal output line.
    pub fn push_terminal_line(&mut self, line: impl Into<String>) {
        self.terminal.push(UiTerminalLine::new(line));
    }

    /// Keep only the most recent terminal output lines.
    pub fn retain_recent_terminal_lines(&mut self, max_lines: usize) {
        if self.terminal.len() > max_lines {
            let remove_count = self.terminal.len() - max_lines;
            self.terminal.drain(0..remove_count);
        }
    }
}

/// One step inside a `UiActivityView`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiActivityStep {
    /// Stable step id used for updates.
    pub id: String,
    /// Visible step label.
    pub label: String,
    /// Current step state.
    pub state: UiActivityStepState,
    /// Optional step detail.
    pub detail: Option<String>,
}

impl UiActivityStep {
    /// Create a pending activity step.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            state: UiActivityStepState::Pending,
            detail: None,
        }
    }

    /// Set the step state.
    pub fn with_state(mut self, state: UiActivityStepState) -> Self {
        self.state = state;
        self
    }

    /// Attach supporting detail to the step.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// State for an activity step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiActivityStepState {
    /// The step has not started.
    Pending,
    /// The step is currently running.
    Active,
    /// The step completed successfully.
    Complete,
    /// The step failed.
    Failed,
}

impl UiActivityStepState {
    /// Return a plain-text marker for non-visual renderers and logs.
    pub fn text_marker(self) -> &'static str {
        match self {
            Self::Pending => "[ ]",
            Self::Active => "[*]",
            Self::Complete => "[x]",
            Self::Failed => "[!]",
        }
    }
}
