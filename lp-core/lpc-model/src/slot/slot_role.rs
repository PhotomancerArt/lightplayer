//! Editing and persistence role attached to slot fields.
//!
//! A role is distinct from [`SlotMeta`](crate::SlotMeta), which describes
//! presentation, and from [`SlotSemantics`](crate::SlotSemantics), which
//! describes resolver-facing dataflow behavior. It is related to — and
//! cross-checked against — a field's
//! [`SlotDirection`](crate::SlotDirection): produced runtime state declares
//! [`SlotRole::State`], and the two must agree (see
//! [`role_matches_direction`]). A produced field is always effectively
//! read-only and never persisted (see [`effective_writable`]) — the other
//! roles govern *authored* fields only.

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
    /// Runtime-produced state: never authored, never client-writable, never
    /// serialized. Declared on every field whose direction is
    /// [`SlotDirection::Produced`], and only on those (G2 amendment —
    /// declaration beats inference; see [`role_matches_direction`]).
    State,
}

impl SlotRole {
    /// True when clients may request mutation of this role's slot.
    pub fn is_writable(self) -> bool {
        !matches!(self, Self::Fixed | Self::State)
    }

    /// Save/writeback hint: true unless the role is [`Self::Debug`] or
    /// [`Self::State`].
    pub fn is_persisted(self) -> bool {
        !matches!(self, Self::Debug | Self::State)
    }

    pub fn is_default(self: &Self) -> bool {
        *self == Self::default()
    }

    /// Stable snake_case token, matching the serde representation and the
    /// `#[slot(role = "...")]` spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Setting => "setting",
            Self::Fixed => "fixed",
            Self::Debug => "debug",
            Self::State => "state",
        }
    }
}

/// Whether a field may declare `role` together with `direction`.
///
/// [`SlotRole::State`] and [`SlotDirection::Produced`] are two spellings of
/// one fact, and the model requires **both**: an unmarked produced field
/// (the old "direction-implied" state record) and a `State` field that is
/// not produced are equally rejected. Enforced at declaration time by the
/// `Slotted` derive when role and direction are both statically visible, and
/// at shape-registration time by
/// [`SlotShape::validate_role_direction`](crate::SlotShape::validate_role_direction)
/// for shapes built at runtime.
pub fn role_matches_direction(role: SlotRole, direction: SlotDirection) -> bool {
    (role == SlotRole::State) == (direction == SlotDirection::Produced)
}

/// Whether a slot governed by `role` accepts client-requested mutation.
///
/// A produced field is always effectively read-only: it is written by its
/// owning node at runtime, so no declared role can make it writable (D1 —
/// direction implies the constraint; since the G2 amendment such a field
/// also declares [`SlotRole::State`], which is read-only in its own right,
/// so the two clauses agree by construction).
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
    fn state_role_is_neither_writable_nor_persisted() {
        assert!(!SlotRole::State.is_writable());
        assert!(!SlotRole::State.is_persisted());
    }

    /// The G2 amendment must be a pure renaming of what direction already
    /// implied: `State` + `Produced` classifies exactly as the unmarked
    /// (`Setting` + `Produced`) state records did before it existed.
    #[test]
    fn state_role_classifies_exactly_like_an_unmarked_produced_field() {
        assert_eq!(
            effective_writable(SlotRole::State, SlotDirection::Produced),
            effective_writable(SlotRole::Setting, SlotDirection::Produced),
        );
        assert!(!effective_writable(
            SlotRole::State,
            SlotDirection::Produced
        ));
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
    fn role_and_direction_must_agree_in_both_directions() {
        assert!(role_matches_direction(
            SlotRole::State,
            SlotDirection::Produced
        ));
        assert!(role_matches_direction(
            SlotRole::Setting,
            SlotDirection::Local
        ));
        assert!(role_matches_direction(
            SlotRole::Debug,
            SlotDirection::Consumed
        ));

        // State without Produced.
        assert!(!role_matches_direction(
            SlotRole::State,
            SlotDirection::Local
        ));
        assert!(!role_matches_direction(
            SlotRole::State,
            SlotDirection::Consumed
        ));
        // Produced without State — the case that made an unmarked
        // `TextureState` possible.
        assert!(!role_matches_direction(
            SlotRole::Setting,
            SlotDirection::Produced
        ));
        assert!(!role_matches_direction(
            SlotRole::Fixed,
            SlotDirection::Produced
        ));
        assert!(!role_matches_direction(
            SlotRole::Debug,
            SlotDirection::Produced
        ));
    }

    #[test]
    fn slot_role_serde_is_snake_case_and_skips_default() {
        let json = serde_json::to_string(&SlotRole::Debug).unwrap();
        assert_eq!(json, "\"debug\"");
        let back: SlotRole = serde_json::from_str(&json).unwrap();
        assert_eq!(back, SlotRole::Debug);

        let json = serde_json::to_string(&SlotRole::State).unwrap();
        assert_eq!(json, "\"state\"");
        let back: SlotRole = serde_json::from_str(&json).unwrap();
        assert_eq!(back, SlotRole::State);
    }
}
