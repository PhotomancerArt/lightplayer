use crate::{StudioView, UxActivity, UxLogEntry, UxNodeId, UxStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UxUpdate {
    View(StudioView),
    Activity {
        node_id: UxNodeId,
        status: UxStatus,
        activity: UxActivity,
    },
    Log(UxLogEntry),
}
