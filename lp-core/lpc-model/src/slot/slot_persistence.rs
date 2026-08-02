//! Derived persistence classification for slot-shaped authored data.
//!
//! Persistence is a tooling/writeback concern: it tells project editors
//! whether a user-editable slot should be saved by default. Unlike the
//! former `SlotPolicy`, it is not itself stored on a slot field shape —
//! [`effective_persistence`] derives it on demand from a field's
//! [`SlotRole`](super::SlotRole) and [`SlotDirection`](super::SlotDirection),
//! so a produced field always classifies as transient regardless of role. It
//! does not affect resolver behavior, dataflow direction, merge policy, or
//! value validation.

use serde::{Deserialize, Serialize};

use super::{SlotDirection, SlotRole};

/// Whether a slot is durable authored data or transient session control data.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SlotPersistence {
    /// Save this slot when writing the authored model.
    #[default]
    Persisted,
    /// User-editable runtime/session control; skip on ordinary save/writeback.
    Transient,
}

impl SlotPersistence {
    pub fn is_persisted(self: &Self) -> bool {
        matches!(self, Self::Persisted)
    }

    /// Classification for an edit whose path resolves in **no** shape —
    /// a stale artifact, an unmounted node, a field the def no longer has.
    ///
    /// Client and server resolve roles independently (the studio walks the
    /// shipped shape snapshot, the registry walks the effective def), so they
    /// must agree on the fallback or the two sides disagree about what an
    /// edit *is*: the studio would hold an invisible live override that the
    /// server silently dropped at commit, or vice versa. The shared answer is
    /// **Setting** (`Persisted`) — the save-relevant default: an
    /// unclassifiable edit presents as authored work and resolves at commit
    /// rather than lingering as a Debug override nothing accounts for.
    pub fn for_unresolved_edit() -> Self {
        Self::Persisted
    }
}

/// Derive the persistence classification governing a field carrying `role`
/// and `direction`: transient unless the role persists (neither
/// [`SlotRole::Debug`] nor [`SlotRole::State`]) and the direction does not
/// imply a produced (never-serialized) field.
pub fn effective_persistence(role: SlotRole, direction: SlotDirection) -> SlotPersistence {
    if role.is_persisted() && direction != SlotDirection::Produced {
        SlotPersistence::Persisted
    } else {
        SlotPersistence::Transient
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_persistence_defaults_to_persisted() {
        assert_eq!(SlotPersistence::default(), SlotPersistence::Persisted);
        assert!(SlotPersistence::default().is_persisted());
    }

    #[test]
    fn slot_persistence_serde_is_snake_case() {
        let json = serde_json::to_string(&SlotPersistence::Transient).unwrap();
        assert_eq!(json, "\"transient\"");
        let back: SlotPersistence = serde_json::from_str(&json).unwrap();
        assert_eq!(back, SlotPersistence::Transient);
    }

    #[test]
    fn produced_direction_is_always_transient() {
        assert_eq!(
            effective_persistence(SlotRole::Setting, SlotDirection::Produced),
            SlotPersistence::Transient
        );
        assert_eq!(
            effective_persistence(SlotRole::Fixed, SlotDirection::Produced),
            SlotPersistence::Transient
        );
    }

    /// The G2 amendment renames what direction implied; it must not move the
    /// classification of a produced field.
    #[test]
    fn state_role_classifies_exactly_like_an_unmarked_produced_field() {
        assert_eq!(
            effective_persistence(SlotRole::State, SlotDirection::Produced),
            effective_persistence(SlotRole::Setting, SlotDirection::Produced),
        );
        assert_eq!(
            effective_persistence(SlotRole::State, SlotDirection::Produced),
            SlotPersistence::Transient
        );
    }

    #[test]
    fn debug_role_is_always_transient() {
        assert_eq!(
            effective_persistence(SlotRole::Debug, SlotDirection::Local),
            SlotPersistence::Transient
        );
    }

    #[test]
    fn setting_role_local_direction_is_persisted() {
        assert_eq!(
            effective_persistence(SlotRole::Setting, SlotDirection::Local),
            SlotPersistence::Persisted
        );
    }
}
