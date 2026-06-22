use crate::{UxActivityStep, UxActivityStepState, UxProgress, UxTerminalLine};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UxActivity {
    pub title: String,
    pub detail: Option<String>,
    pub progress: Option<UxProgress>,
    pub steps: Vec<UxActivityStep>,
    pub terminal: Vec<UxTerminalLine>,
}

impl UxActivity {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            detail: None,
            progress: None,
            steps: Vec::new(),
            terminal: Vec::new(),
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_progress(mut self, progress: UxProgress) -> Self {
        self.progress = Some(progress);
        self
    }

    pub fn with_steps(mut self, steps: Vec<UxActivityStep>) -> Self {
        self.steps = steps;
        self
    }

    pub fn set_step_state(&mut self, id: &str, state: UxActivityStepState) {
        if let Some(step) = self.steps.iter_mut().find(|step| step.id == id) {
            step.state = state;
        }
    }

    pub fn push_terminal_line(&mut self, line: impl Into<String>) {
        self.terminal.push(UxTerminalLine::new(line));
    }

    pub fn retain_recent_terminal_lines(&mut self, max_lines: usize) {
        if self.terminal.len() > max_lines {
            let remove_count = self.terminal.len() - max_lines;
            self.terminal.drain(0..remove_count);
        }
    }
}
