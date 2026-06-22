use crate::{ProgressState, UiActivity, UiMetric, UxIssue};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiBody {
    Empty,
    Text(String),
    Progress(ProgressState),
    Activity(UiActivity),
    Issue(UxIssue),
    Metrics(Vec<UiMetric>),
}

impl UiBody {
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

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
                lines.extend(
                    activity
                        .terminal
                        .iter()
                        .rev()
                        .take(8)
                        .map(|line| line.text.clone()),
                );
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
        }
    }
}
