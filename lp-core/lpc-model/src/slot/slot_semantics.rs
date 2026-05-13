//! Resolver-facing semantics attached to slot fields.

use serde::{Deserialize, Serialize};

use crate::{SlotDirection, SlotMerge};

/// Behavioral metadata for a slot field.
///
/// [`SlotSemantics`] is part of the authoritative slot shape. It describes how
/// the field participates in dataflow, not how it should be displayed. UI labels
/// and authoring hints belong in [`crate::SlotMeta`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotSemantics {
    #[serde(default, skip_serializing_if = "SlotDirection::is_local")]
    pub direction: SlotDirection,
    #[serde(default, skip_serializing_if = "SlotMerge::is_latest")]
    pub merge: SlotMerge,
}

impl SlotSemantics {
    pub const fn new(direction: SlotDirection, merge: SlotMerge) -> Self {
        Self { direction, merge }
    }

    pub const fn local() -> Self {
        Self::new(SlotDirection::Local, SlotMerge::Latest)
    }

    pub const fn consumed(merge: SlotMerge) -> Self {
        Self::new(SlotDirection::Consumed, merge)
    }

    pub const fn produced() -> Self {
        Self::new(SlotDirection::Produced, SlotMerge::Latest)
    }
}

impl Default for SlotSemantics {
    fn default() -> Self {
        Self::local()
    }
}

impl SlotDirection {
    pub fn is_local(self: &Self) -> bool {
        matches!(self, Self::Local)
    }
}

impl SlotMerge {
    pub fn is_latest(self: &Self) -> bool {
        matches!(self, Self::Latest)
    }
}
