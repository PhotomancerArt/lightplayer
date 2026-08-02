//! Editing and persistence role attached to slot fields.
//!
//! A role is distinct from [`SlotMeta`](crate::SlotMeta), which describes
//! presentation, and from [`SlotSemantics`](crate::SlotSemantics), which
//! describes resolver-facing dataflow behavior. It is also distinct from a
//! field's [`SlotDirection`](crate::SlotDirection): a produced field is
//! always effectively read-only and never persisted regardless of its role
//! (see [`effective_writable`]) — role governs *authored* fields only.

use serde::{Deserialize, Serialize};

use super::SlotDirection;

/// Client mutation and persistence role for one slot field.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SlotRole {
    /// Authored node config: writable, persisted. The default.
    #[default]
    Setting,
    /// Read-only persisted authored data (today: three fields on
    /// `ProjectDef`).
    Fixed,
    /// Writable, transient by nature: diagnostics/authoring overrides. Never
    /// serialized; lives only in the session overlay.
    Debug,
}

impl SlotRole {
    /// True when clients may request mutation of this role's slot.
    pub fn is_writable(self) -> bool {
        !matches!(self, Self::Fixed)
    }

    /// Save/writeback hint: true unless the role is [`Self::Debug`].
    pub fn is_persisted(self) -> bool {
        !matches!(self, Self::Debug)
    }

    pub fn is_default(self: &Self) -> bool {
        *self == Self::default()
    }
}

/// Whether a slot governed by `role` accepts client-requested mutation.
///
/// A produced field is always effectively read-only: it is written by its
/// owning node at runtime, so no declared role can make it writable (D1 —
/// direction implies the constraint; the former `read_only_transient`
/// marking on state records is gone because of it).
pub fn effective_writable(role: SlotRole, direction: SlotDirection) -> bool {
    role.is_writable() && direction != SlotDirection::Produced
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_role_defaults_to_setting() {
        assert_eq!(SlotRole::default(), SlotRole::Setting);
        assert!(SlotRole::default().is_writable());
        assert!(SlotRole::default().is_persisted());
    }

    #[test]
    fn debug_role_is_writable_but_not_persisted() {
        assert!(SlotRole::Debug.is_writable());
        assert!(!SlotRole::Debug.is_persisted());
    }

    #[test]
    fn fixed_role_is_persisted_but_not_writable() {
        assert!(!SlotRole::Fixed.is_writable());
        assert!(SlotRole::Fixed.is_persisted());
    }

    #[test]
    fn produced_direction_is_never_effectively_writable() {
        assert!(!effective_writable(
            SlotRole::Setting,
            SlotDirection::Produced
        ));
        assert!(!effective_writable(
            SlotRole::Debug,
            SlotDirection::Produced
        ));
        assert!(effective_writable(SlotRole::Setting, SlotDirection::Local));
    }

    #[test]
    fn slot_role_serde_is_snake_case_and_skips_default() {
        let json = serde_json::to_string(&SlotRole::Debug).unwrap();
        assert_eq!(json, "\"debug\"");
        let back: SlotRole = serde_json::from_str(&json).unwrap();
        assert_eq!(back, SlotRole::Debug);
    }
}
