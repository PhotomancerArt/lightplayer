use crate::{UxProgress, UxTerminalLine};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UxActivity {
    pub title: String,
    pub detail: Option<String>,
    pub progress: Option<UxProgress>,
    pub terminal: Vec<UxTerminalLine>,
}

impl UxActivity {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            detail: None,
            progress: None,
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
