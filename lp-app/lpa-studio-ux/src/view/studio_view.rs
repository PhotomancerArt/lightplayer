use core::fmt::Write;

use crate::{ActionPriority, UxLogEntry, UxPaneView};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StudioView {
    pub panes: Vec<UxPaneView>,
    pub logs: Vec<UxLogEntry>,
}

impl StudioView {
    pub fn new(panes: Vec<UxPaneView>, logs: Vec<UxLogEntry>) -> Self {
        Self { panes, logs }
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
        for pane in &self.panes {
            let _ = writeln!(output, "{}", pane.title);
            let _ = writeln!(output, "  node: {}", pane.node_id);
            let _ = writeln!(output, "  status: {}", pane.status.label);
            for line in pane.body.render_text_lines() {
                let _ = writeln!(output, "  {line}");
            }
            if !pane.actions.is_empty() {
                let _ = writeln!(output, "  actions:");
                for action in &pane.actions {
                    let meta = action.meta();
                    let _ = writeln!(
                        output,
                        "    - [{}] {}",
                        priority_label(meta.priority),
                        meta.label
                    );
                }
            }
            output.push('\n');
        }
        if !self.logs.is_empty() {
            let _ = writeln!(output, "Runtime");
            for log in self.logs.iter().rev().take(8) {
                let _ = writeln!(output, "  {:?} {}: {}", log.level, log.source, log.message);
            }
        }
        output
    }
}

fn priority_label(priority: ActionPriority) -> &'static str {
    match priority {
        ActionPriority::Primary => "primary",
        ActionPriority::Secondary => "secondary",
        ActionPriority::Tertiary => "tertiary",
    }
}
