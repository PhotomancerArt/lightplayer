//! What values are **legal** in a slot: the domain’s validation truth, *not* a
//! parallel “UI only” model (`docs/design/lightplayer/quantity.md` §5).
//!
//! A [`Constraint`] **refines** the natural domain of a [`Kind`](crate::kind::Kind)
//! (e.g. [`Kind::Amplitude`](crate::kind::Kind::Amplitude) defaults to
//! a unit range, but a slot can tighten, widen, or set [`Constraint::Free`]
//! for “boost”-style use (`quantity.md` §5). Bindings and loaded values that
//! violate a slot’s constraint are **compose-time errors** (same section).
//! Color coordinates default to [`Constraint::Free`] in the spec so
//! out-of-gamut/boost stays meaningful; a slot that needs strict in-gamut
//! authoring can override with [`Constraint::Range`] (`color.md` pointer in
//! `quantity.md` §5).
//!
//! v0 **narrows** range and choice payloads to `f32` so this enum derives
//! `serde`/`JsonSchema` without `LpsValue` carrying serde in `lps-shared` yet.
//! A future **widening** to `LpsValue` is recorded as intent in
//! `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/summary.md` (“F32
//! narrowing in v0”).

use alloc::string::String;
use alloc::vec::Vec;

/// Declares which [`crate::LpsValue`]s are *allowed* for a slot (together with
/// its [`Kind`][`crate::kind::Kind`]). Serialize shape uses `type` tagging with
/// snake_case variant names in JSON.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Constraint {
    /// No static bound beyond what the kind implies; the slot accepts any
    /// in-range for its storage type, subject to later validation
    /// (`docs/design/lightplayer/quantity.md` §5, `Color` uses this by default
    /// for coords).
    Free,
    /// Inclusive min/max, optional discrete **step** for snapping (UI may show
    /// a stepped control; the constraint is still domain truth, `quantity.md` §5).
    Range {
        min: f32,
        max: f32,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        step: Option<f32>,
    },
    /// Discrete choices: parallel `values` and string `labels` for the same
    /// indices (dropdowns use labels; the bound value must be one of `values`,
    /// `quantity.md` §5 sketch).
    Choice {
        values: Vec<f32>,
        labels: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_constraint_round_trips() {
        let c = Constraint::Free;
        let s = serde_json::to_string(&c).unwrap();
        let back: Constraint = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn range_constraint_round_trips() {
        let c = Constraint::Range {
            min: 0.0,
            max: 5.0,
            step: Some(0.1),
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: Constraint = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn range_omits_step_when_none() {
        let c = Constraint::Range {
            min: 0.0,
            max: 1.0,
            step: None,
        };
        let s = serde_json::to_string(&c).unwrap();
        assert!(!s.contains("step"));
    }

    #[test]
    fn choice_round_trips() {
        let c = Constraint::Choice {
            values: alloc::vec![0.0, 1.0, 2.0],
            labels: alloc::vec![
                String::from("low"),
                String::from("med"),
                String::from("high"),
            ],
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: Constraint = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }
}
