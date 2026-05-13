//! Persistence hints for slot-shaped authored data.
//!
//! Persistence is a tooling/writeback concern. It tells project editors whether
//! a user-editable slot should be saved by default. It does not affect resolver
//! behavior, dataflow direction, merge policy, or value validation.

use serde::{Deserialize, Serialize};

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
}
