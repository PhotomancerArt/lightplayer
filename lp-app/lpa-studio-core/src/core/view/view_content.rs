use crate::{
    ProjectEditorView, UiActivityView, UiBusView, UiIssue, UiMetric, UiProgress, UiStepsView,
};

/// Generic body content for panes and workflow steps.
///
/// This enum lets controllers describe common renderable content without
/// choosing web components directly. Keep app-specific surfaces in app view
/// DTOs and use these variants for reusable body shapes.
#[derive(Clone, Debug, PartialEq)]
pub enum UiViewContent {
    /// No visible body content.
    Empty,
    /// A single paragraph of text.
    Text(String),
    /// Progress for ongoing work.
    Progress(UiProgress),
    /// A multi-step activity.
    Activity(UiActivityView),
    /// An inline problem that needs attention.
    Issue(UiIssue),
    /// A compact label/value metric grid.
    Metrics(Vec<UiMetric>),
    /// A composed workflow with ordered steps.
    Stack(Box<UiStepsView>),
    /// Project editor surface.
    ProjectEditor(Box<ProjectEditorView>),
    /// Bus channel surface.
    Bus(Box<UiBusView>),
}

impl UiViewContent {
    /// Create text body content.
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    /// Render the body as plain text lines for fallback renderers and tests.
    pub fn render_text_lines(&self) -> Vec<String> {
        match self {
            Self::Empty => Vec::new(),
            Self::Text(text) => vec![text.clone()],
            Self::Progress(progress) => match &progress.detail {
                Some(detail) => vec![progress.label.clone(), detail.clone()],
                None => vec![progress.label.clone()],
            },
            Self::Activity(activity) => {
                let mut lines = vec![activity.title.clone()];
                if let Some(detail) = &activity.detail {
                    lines.push(detail.clone());
                }
                if let Some(progress) = &activity.progress {
                    lines.push(progress.label.clone());
                    if let Some(detail) = &progress.detail {
                        lines.push(detail.clone());
                    }
                }
                lines.extend(activity.steps.iter().map(|step| {
                    let line = format!("{} {}", step.state.text_marker(), step.label);
                    match &step.detail {
                        Some(detail) => format!("{line}: {detail}"),
                        None => line,
                    }
                }));
                lines
            }
            Self::Issue(issue) => match &issue.detail {
                Some(detail) => vec![issue.message.clone(), detail.clone()],
                None => vec![issue.message.clone()],
            },
            Self::Metrics(metrics) => metrics
                .iter()
                .map(|metric| format!("{}: {}", metric.label, metric.value))
                .collect(),
            Self::Stack(stack) => {
                let mut lines = Vec::new();
                for section in &stack.sections {
                    lines.push(format!("{} {}", section.state.text_marker(), section.title));
                    lines.extend(
                        section
                            .body
                            .render_text_lines()
                            .into_iter()
                            .map(|line| format!("  {line}")),
                    );
                    if !section.actions.is_empty() {
                        lines.push("  actions:".to_string());
                        lines.extend(
                            section
                                .actions
                                .iter()
                                .map(|action| format!("    - {}", action.meta().label)),
                        );
                    }
                }
                if !stack.terminal.is_empty() {
                    lines.push("terminal:".to_string());
                    lines.extend(
                        stack
                            .terminal
                            .iter()
                            .rev()
                            .take(12)
                            .rev()
                            .map(|line| format!("  {}", line.text)),
                    );
                }
                lines
            }
            Self::Bus(bus) => {
                let mut lines = vec![format!("Channels: {}", bus.channels.len())];
                for channel in &bus.channels {
                    let value = channel.value.as_deref().unwrap_or("—");
                    lines.push(format!(
                        "{} = {} (writers {}, readers {})",
                        channel.name,
                        value,
                        channel.writers.len(),
                        channel.readers.len()
                    ));
                }
                lines
            }
            Self::ProjectEditor(editor) => {
                let mut lines = vec![
                    format!("Project: {}", editor.project_id),
                    format!("Nodes: {}", editor.nodes.len()),
                ];
                for node in &editor.nodes {
                    lines.push(format!(
                        "{} {} {}",
                        node.node_id, node.header.kind, node.header.path
                    ));
                    for tab in &node.tabs {
                        if let crate::UiNodeTabBody::Sections(sections) = &tab.body {
                            lines.extend(sections.iter().map(|section| {
                                let label = match section {
                                    crate::UiNodeSection::ProducedProducts(items) => {
                                        format!("produced products: {}", items.len())
                                    }
                                    crate::UiNodeSection::ProducedValues(items) => {
                                        format!("produced values: {}", items.len())
                                    }
                                    crate::UiNodeSection::ConfigSlots(items) => {
                                        format!("config slots: {}", items.len())
                                    }
                                    crate::UiNodeSection::AssetSlots(items) => {
                                        format!("asset slots: {}", items.len())
                                    }
                                    crate::UiNodeSection::Children(items) => {
                                        format!("children: {}", items.len())
                                    }
                                };
                                format!("  {label}")
                            }));
                        }
                    }
                }
                lines
            }
        }
    }
}
