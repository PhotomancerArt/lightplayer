use alloc::string::String;

use lpc_model::SlotPath;

/// Upsert identity for one pending [`super::SlotEdit`] in an overlay.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PendingSlotTarget {
    /// Leaf, enum variant, or option at this path.
    Slot(SlotPath),
    MapInsert {
        path: SlotPath,
        key: String,
    },
    MapRemove {
        path: SlotPath,
        key: String,
    },
}
