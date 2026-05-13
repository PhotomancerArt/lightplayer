//! Tooling and mutation policy attached to slot fields.
//!
//! Policy is distinct from [`SlotMeta`](crate::SlotMeta), which describes
//! presentation, and from [`SlotSemantics`](crate::SlotSemantics), which
//! describes resolver-facing dataflow behavior.

use serde::{Deserialize, Serialize};

use super::SlotPersistence;

/// Client mutation and persistence policy for one slot field.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotPolicy {
    /// True when clients may request mutation of this slot.
    #[serde(default = "default_writable", skip_serializing_if = "is_true")]
    pub writable: bool,

    /// Save/writeback hint for user-editable slot data.
    #[serde(default, skip_serializing_if = "SlotPersistence::is_persisted")]
    pub persistence: SlotPersistence,
}

impl SlotPolicy {
    /// Read-only persisted authored data.
    pub const fn read_only_persisted() -> Self {
        Self {
            writable: false,
            persistence: SlotPersistence::Persisted,
        }
    }

    /// Writable persisted authored data.
    pub const fn writable_persisted() -> Self {
        Self {
            writable: true,
            persistence: SlotPersistence::Persisted,
        }
    }

    /// Read-only transient data.
    pub const fn read_only_transient() -> Self {
        Self {
            writable: false,
            persistence: SlotPersistence::Transient,
        }
    }

    /// Writable transient user control data.
    pub const fn writable_transient() -> Self {
        Self {
            writable: true,
            persistence: SlotPersistence::Transient,
        }
    }

    pub fn is_default(self: &Self) -> bool {
        *self == Self::default()
    }
}

impl Default for SlotPolicy {
    fn default() -> Self {
        Self::writable_persisted()
    }
}

fn default_writable() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_policy_defaults_to_writable_persisted() {
        assert_eq!(SlotPolicy::default(), SlotPolicy::writable_persisted());
    }

    #[test]
    fn writable_transient_policy_round_trips() {
        let policy = SlotPolicy::writable_transient();
        let json = serde_json::to_string(&policy).unwrap();
        assert!(!json.contains("writable"));
        assert!(json.contains("transient"));
        let back: SlotPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back, policy);
    }
}
