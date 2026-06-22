use crate::{StudioView, UiActivity, UiStatus, UxLogEntry, UxNodeId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UxUpdate {
    View(StudioView),
    Activity {
        node_id: UxNodeId,
        status: UiStatus,
        activity: UiActivity,
    },
    Log(UxLogEntry),
}
