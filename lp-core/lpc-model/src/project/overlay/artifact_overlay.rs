//! Canonical pending edits for one artifact.

use super::{AssetOverlay, SlotEdit, SlotOverlay};

/// Current pending intent for one artifact.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ArtifactOverlay {
    Slot { overlay: SlotOverlay },
    Asset { overlay: AssetOverlay },
}

impl ArtifactOverlay {
    pub fn slot(overlay: SlotOverlay) -> Self {
        Self::Slot { overlay }
    }

    pub fn body(edit: AssetOverlay) -> Self {
        Self::Asset { overlay: edit }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Slot { overlay } if overlay.is_empty())
    }

    pub fn as_slot(&self) -> Option<&SlotOverlay> {
        match self {
            Self::Slot { overlay } => Some(overlay),
            Self::Asset { .. } => None,
        }
    }

    pub fn as_body(&self) -> Option<&AssetOverlay> {
        match self {
            Self::Slot { .. } => None,
            Self::Asset { overlay: edit } => Some(edit),
        }
    }

    pub fn put_slot_edit(&mut self, edit: SlotEdit) -> bool {
        match self {
            Self::Slot { overlay } => overlay.put_edit(edit),
            Self::Asset { .. } => {
                let mut overlay = SlotOverlay::new();
                overlay.put_edit(edit);
                *self = Self::Slot { overlay };
                true
            }
        }
    }
}
