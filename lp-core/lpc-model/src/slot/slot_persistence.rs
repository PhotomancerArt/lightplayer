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
    /// Save this slot when writing the authored model unless another policy overrides it.
    #[default]
    Persisted,
    /// User-editable runtime/session control; skip on ordinary save/writeback.
    Transient,
}

impl SlotPersistence {
    pub fn is_persisted(self: &Self) -> bool {
        matches!(self, Self::Persisted)
    }
}

/// Derive the persistence classification governing a field carrying `role`
/// and `direction`: transient unless the role persists (not [`SlotRole::Debug`])
/// and the direction does not imply a produced (never-serialized) field.
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
