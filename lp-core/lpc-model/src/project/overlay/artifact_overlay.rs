use super::{AssetBodyOverlay, SlotEdit, SlotOverlay};

/// Current pending intent for one artifact.
///
/// An artifact overlay is exclusive: an artifact is either edited structurally
/// through slot edits or edited as a whole asset body. Switching between those
/// modes replaces the previous pending intent for that artifact.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactOverlay {
    /// Structured edits to an authored node-definition artifact.
    Slot { overlay: SlotOverlay },
    /// Whole-body edit to an asset or definition artifact.
    Asset { overlay: AssetBodyOverlay },
}

impl ArtifactOverlay {
    pub fn slot(overlay: SlotOverlay) -> Self {
        Self::Slot { overlay }
    }

    pub fn body(edit: AssetBodyOverlay) -> Self {
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

    pub fn as_body(&self) -> Option<&AssetBodyOverlay> {
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
