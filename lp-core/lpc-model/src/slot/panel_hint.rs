//! Authored panel-visibility hint on a slot declaration.

use serde::{Deserialize, Serialize};

/// An additive override on the derived panel-membership rule
/// (ADR 2026-08-03-panel-visibility-is-derived, amended): `Show` promotes
/// the binding materialized from the slot's own `default_bind` to
/// publicity, so the control appears even though the wiring is
/// Default-origin (a fixture's brightness fader). Absent means the derived
/// rule alone decides — authored wiring is public, default wiring is not.
///
/// There is deliberately no `Hide`: suppressing AUTHORED wiring is
/// module-level curation (the deferred authored panel layouts), not a
/// kind-level veto, and a hint that can silently override an author's
/// binding is the deleted `panel: bool` flag growing back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PanelHint {
    /// The slot's default-bound channel presents a panel control.
    Show,
}

impl PanelHint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Show => "show",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "show" => Some(Self::Show),
            _ => None,
        }
    }
}
