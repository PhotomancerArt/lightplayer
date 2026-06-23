#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectSlotRowView {
    Value(ProjectSlotValueView),
    Group(ProjectSlotGroupView),
    Issue(ProjectSlotIssueView),
}

impl ProjectSlotRowView {
    pub fn value(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self::Value(ProjectSlotValueView::new(label, value, None))
    }

    pub fn value_with_detail(
        label: impl Into<String>,
        value: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::Value(ProjectSlotValueView::new(label, value, Some(detail.into())))
    }

    pub fn group(
        label: impl Into<String>,
        detail: Option<String>,
        rows: Vec<ProjectSlotRowView>,
    ) -> Self {
        Self::Group(ProjectSlotGroupView::new(label, detail, rows))
    }

    pub fn issue(label: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Issue(ProjectSlotIssueView::new(label, message))
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Value(row) => &row.label,
            Self::Group(row) => &row.label,
            Self::Issue(row) => &row.label,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSlotValueView {
    pub label: String,
    pub value: String,
    pub detail: Option<String>,
}

impl ProjectSlotValueView {
    pub fn new(label: impl Into<String>, value: impl Into<String>, detail: Option<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            detail,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSlotGroupView {
    pub label: String,
    pub detail: Option<String>,
    pub rows: Vec<ProjectSlotRowView>,
}

impl ProjectSlotGroupView {
    pub fn new(
        label: impl Into<String>,
        detail: Option<String>,
        rows: Vec<ProjectSlotRowView>,
    ) -> Self {
        Self {
            label: label.into(),
            detail,
            rows,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSlotIssueView {
    pub label: String,
    pub message: String,
}

impl ProjectSlotIssueView {
    pub fn new(label: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            message: message.into(),
        }
    }
}
