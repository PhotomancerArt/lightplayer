//! What values are **legal** in a slot: the domain’s validation truth, *not* a
//! parallel “UI only” model (`docs/design/lightplayer/quantity.md` §5).
//!
//! Variants are discriminated by which **peer key** is present in the serialized
//! table (no `type = "..."` tag). `range = [a, b]` ⇒ range form;
//! `choices = [...]` ⇒ choice form; neither ⇒ free form.
//! Each form is a dedicated struct with `deny_unknown_fields` so typos are hard
//! errors; the outer [`Constraint`] enum is `#[serde(untagged)]` to merge those
//! shapes (`docs/plans/2026-04-22-lp-domain-m3-visual-artifact-types/00-design.md`).
//!
//! A [`Constraint`] **refines** the natural domain of a [`Kind`](crate::kind::Kind)
//! (e.g. [`Kind::Amplitude`](crate::kind::Kind::Amplitude) defaults to
//! a unit range, but a slot can tighten, widen, or set [`Constraint::Free`]
//! for “boost”-style use (`quantity.md` §5). Bindings and loaded values that
//! violate a slot’s constraint are **compose-time errors** (same section).
//! Color coordinates default to [`Constraint::Free`] in the spec so
//! out-of-gamut/boost stays meaningful; a slot that needs strict in-gamut
//! authoring can override with range form (`color.md` pointer in
//! `quantity.md` §5).
//!
//! v0 **narrows** range and choice payloads to `f32` so this enum derives
//! `serde`/`JsonSchema` without `LpsValue` carrying serde in `lps-shared` yet.
//! A future **widening** to `LpsValue` is recorded as intent in
//! `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/summary.md` (“F32
//! narrowing in v0”).

use alloc::string::String;
use alloc::vec::Vec;

/// Inclusive `[min, max]` with optional discrete **step** (`quantity.md` §5).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)] // Mutex flat keys; typos → hard errors per 00-design.md §Constraint.
pub struct ConstraintRange {
    pub range: [f32; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f32>,
}

/// Discrete choices: parallel `choices` and `labels` (`quantity.md` §5).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ConstraintChoice {
    pub choices: Vec<f32>,
    pub labels: Vec<String>,
}

/// No static bound beyond what the kind implies; empty table / object.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ConstraintFree {}

/// Declares which [`crate::LpsValue`]s are *allowed* for a slot (together with
/// its [`Kind`][`crate::kind::Kind`]). On-disk shape uses **peer-key**
/// inference (`#[serde(untagged)]`): see module docs.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum Constraint {
    Range(ConstraintRange),
    Choice(ConstraintChoice),
    Free(ConstraintFree),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_round_trips_as_empty_object() {
        let c = Constraint::Free(ConstraintFree {});
        let s = serde_json::to_string(&c).unwrap();
        assert_eq!(s, "{}");
        let back: Constraint = serde_json::from_str("{}").unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn range_constraint_round_trips() {
        let c = Constraint::Range(ConstraintRange {
            range: [0.0, 5.0],
            step: Some(0.1),
        });
        let s = serde_json::to_string(&c).unwrap();
        let back: Constraint = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn range_emits_array_form() {
        let c = Constraint::Range(ConstraintRange {
            range: [0.0, 1.0],
            step: None,
        });
        let s = serde_json::to_string(&c).unwrap();
        assert!(s.contains("\"range\":[0.0,1.0]"), "got {s}");
        assert!(!s.contains("step"));
    }

    #[test]
    fn choice_round_trips() {
        let c = Constraint::Choice(ConstraintChoice {
            choices: alloc::vec![0.0, 1.0, 2.0],
            labels: alloc::vec![
                String::from("low"),
                String::from("med"),
                String::from("high"),
            ],
        });
        let s = serde_json::to_string(&c).unwrap();
        let back: Constraint = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn unknown_field_in_range_is_rejected() {
        let res: Result<Constraint, _> = serde_json::from_str(r#"{"range":[0,1],"setp":0.1}"#);
        assert!(res.is_err());
    }

    #[test]
    fn range_loads_from_toml() {
        let c: Constraint = toml::from_str("range = [0, 5]\nstep = 1\n").unwrap();
        match c {
            Constraint::Range(ConstraintRange { range, step }) => {
                assert_eq!(range, [0.0, 5.0]);
                assert_eq!(step, Some(1.0));
            }
            _ => panic!("expected Range"),
        }
    }
}
