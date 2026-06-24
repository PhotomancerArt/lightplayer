use crate::{ProjectNodeStatusView, ProjectSlotRowView, UiAction};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectNodeView {
    pub node_id: String,
    pub label: String,
    pub kind: String,
    pub path: String,
    pub status: ProjectNodeStatusView,
    pub focused: bool,
    pub action: UiAction,
    pub prominent_slots: Vec<ProjectSlotRowView>,
    pub config_slots: Vec<ProjectSlotRowView>,
    pub state_slots: Vec<ProjectSlotRowView>,
    pub binding_slots: Vec<ProjectSlotRowView>,
    pub issues: Vec<String>,
}

impl ProjectNodeView {
    #[allow(
        clippy::too_many_arguments,
        reason = "project node views are dumb data returned to UI renderers"
    )]
    pub fn new(
        node_id: impl Into<String>,
        label: impl Into<String>,
        kind: impl Into<String>,
        path: impl Into<String>,
        status: ProjectNodeStatusView,
        focused: bool,
        action: UiAction,
        prominent_slots: Vec<ProjectSlotRowView>,
        config_slots: Vec<ProjectSlotRowView>,
        state_slots: Vec<ProjectSlotRowView>,
        binding_slots: Vec<ProjectSlotRowView>,
        issues: Vec<String>,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            label: label.into(),
            kind: kind.into(),
            path: path.into(),
            status,
            focused,
            action,
            prominent_slots,
            config_slots,
            state_slots,
            binding_slots,
            issues,
        }
    }

    pub fn has_slots(&self) -> bool {
        !(self.prominent_slots.is_empty()
            && self.config_slots.is_empty()
            && self.state_slots.is_empty()
            && self.binding_slots.is_empty())
    }
}
